use std::io::{BufRead, Write};
use std::net::TcpListener;
use std::sync::Mutex;

use crate::domain::OAuthCallback;
use crate::error::GcalError;
use crate::ports::AuthCodeReceiver;

/// ローカル HTTP サーバーで OAuth2 コールバックを受け取る
pub struct LoopbackReceiver {
    listener: TcpListener,
}

impl LoopbackReceiver {
    /// エフェメラルポートでリスナーを作成する
    pub fn bind() -> Result<Self, GcalError> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        Ok(Self { listener })
    }

    /// リスナーがバインドされているポートを返す
    pub fn port(&self) -> u16 {
        self.listener.local_addr().unwrap().port()
    }
}

impl AuthCodeReceiver for LoopbackReceiver {
    fn redirect_uri(&self) -> String {
        format!("http://127.0.0.1:{}/callback", self.port())
    }

    fn receive_code(&self) -> Result<OAuthCallback, GcalError> {
        let (stream, _) = self.listener.accept()?;
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(120)))
            .ok();

        // まず request line を読む（reader を別スコープで扱い、stream の借用を終了させる）
        let request_line = {
            let mut reader = std::io::BufReader::new(&stream);
            let mut line = String::new();
            reader.read_line(&mut line)?;

            // 残りのヘッダーを読み捨てる（コネクションリセットを防ぐ）
            let mut header = String::new();
            loop {
                header.clear();
                let n = reader.read_line(&mut header)?;
                if n == 0 || header == "\r\n" || header == "\n" {
                    break;
                }
            }
            line
        }; // ここで reader の借用が終わる

        let callback = parse_callback_from_request_line(&request_line)?;

        // ブラウザに成功レスポンスを返す（writer を別スコープで扱う）
        {
            let body = "<html><body><h1>認証完了</h1><p>このタブを閉じてください。</p></body></html>";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let mut writer = std::io::BufWriter::new(&stream);
            writer.write_all(response.as_bytes())?;
            writer.flush()?;
        }

        Ok(callback)
    }
}

fn parse_callback_from_request_line(line: &str) -> Result<OAuthCallback, GcalError> {
    // "GET /callback?code=xxx&state=yyy HTTP/1.1"
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 2 {
        return Err(GcalError::AuthError("無効なコールバックリクエスト".to_string()));
    }

    let path_and_query = parts[1];
    let query_str = path_and_query
        .split_once('?')
        .map(|(_, q)| q)
        .unwrap_or("");

    parse_callback_from_query(query_str)
}

fn parse_callback_from_query(query_str: &str) -> Result<OAuthCallback, GcalError> {
    let mut code = None;
    let mut state = None;

    for param in query_str.split('&') {
        if let Some((key, value)) = param.split_once('=') {
            match key {
                "code" => code = Some(url_decode(value)),
                "state" => state = Some(url_decode(value)),
                "error" => {
                    return Err(GcalError::AuthError(format!("OAuth エラー: {value}")));
                }
                _ => {}
            }
        }
    }

    match (code, state) {
        (Some(code), Some(state)) => Ok(OAuthCallback { code, state }),
        _ => Err(GcalError::AuthError(
            "URL に code または state がありません".to_string(),
        )),
    }
}

/// 簡易 URL デコード（%XX を文字に変換）
fn url_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            let h1 = chars.next().unwrap_or('0');
            let h2 = chars.next().unwrap_or('0');
            if let Ok(byte) = u8::from_str_radix(&format!("{h1}{h2}"), 16) {
                result.push(byte as char);
            }
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }
    result
}

/// 手動入力でコールバック URL を受け取る（SSH 環境向けフォールバック）
pub struct ManualReceiver<R: BufRead> {
    reader: Mutex<R>,
}

impl<R: BufRead> ManualReceiver<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader: Mutex::new(reader),
        }
    }
}

impl<R: BufRead + Send + Sync> AuthCodeReceiver for ManualReceiver<R> {
    fn redirect_uri(&self) -> String {
        // 手動フローでは OOB（Out-of-Band）相当: ユーザーが URL を貼り付ける
        "urn:ietf:wg:oauth:2.0:oob".to_string()
    }

    fn receive_code(&self) -> Result<OAuthCallback, GcalError> {
        let mut line = String::new();
        self.reader.lock().unwrap().read_line(&mut line)?;
        let trimmed = line.trim();

        // "http://127.0.0.1:PORT/callback?code=xxx&state=yyy" 形式を想定
        let query_str = if let Some(pos) = trimmed.find('?') {
            &trimmed[pos + 1..]
        } else {
            trimmed
        };

        parse_callback_from_query(query_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_parse_callback_from_request_line_ok() {
        let line = "GET /callback?code=auth_code_123&state=csrf_xyz HTTP/1.1\r\n";
        let cb = parse_callback_from_request_line(line).unwrap();
        assert_eq!(cb.code, "auth_code_123");
        assert_eq!(cb.state, "csrf_xyz");
    }

    #[test]
    fn test_parse_callback_from_request_line_missing_code() {
        let line = "GET /callback?state=csrf_xyz HTTP/1.1\r\n";
        let result = parse_callback_from_request_line(line);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_callback_from_request_line_oauth_error() {
        let line = "GET /callback?error=access_denied&state=xyz HTTP/1.1\r\n";
        let result = parse_callback_from_request_line(line);
        assert!(matches!(result, Err(GcalError::AuthError(_))));
    }

    #[test]
    fn test_manual_receiver_from_full_url() {
        let input = "http://127.0.0.1:8888/callback?code=mycode&state=mystate\n";
        let reader = Cursor::new(input);
        let receiver = ManualReceiver::new(reader);
        let cb = receiver.receive_code().unwrap();
        assert_eq!(cb.code, "mycode");
        assert_eq!(cb.state, "mystate");
    }

    #[test]
    fn test_manual_receiver_from_query_only() {
        let input = "code=qcode&state=qstate\n";
        let reader = Cursor::new(input);
        let receiver = ManualReceiver::new(reader);
        let cb = receiver.receive_code().unwrap();
        assert_eq!(cb.code, "qcode");
        assert_eq!(cb.state, "qstate");
    }

    #[test]
    fn test_manual_receiver_redirect_uri() {
        let reader = Cursor::new("");
        let receiver = ManualReceiver::new(reader);
        assert_eq!(receiver.redirect_uri(), "urn:ietf:wg:oauth:2.0:oob");
    }

    #[test]
    fn test_url_decode_percent_encoding() {
        assert_eq!(url_decode("hello%20world"), "hello world");
        assert_eq!(url_decode("a+b"), "a b");
        assert_eq!(url_decode("plain"), "plain");
    }

    #[test]
    fn test_loopback_receiver_binds_port() {
        let receiver = LoopbackReceiver::bind().unwrap();
        assert!(receiver.port() > 0);
        assert!(receiver.redirect_uri().contains("127.0.0.1"));
    }

    #[test]
    fn test_loopback_receiver_receive_code() {
        use std::io::{Read, Write};

        let receiver = LoopbackReceiver::bind().unwrap();
        let port = receiver.port();

        let handle = std::thread::spawn(move || {
            let mut stream = std::net::TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
            let request = "GET /callback?code=loopback_code&state=loopback_state HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
            stream.write_all(request.as_bytes()).unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            response
        });

        let cb = receiver.receive_code().unwrap();
        let response = handle.join().unwrap();

        assert_eq!(cb.code, "loopback_code");
        assert_eq!(cb.state, "loopback_state");
        assert!(response.contains("HTTP/1.1 200 OK"), "レスポンス: {response}");
        assert!(response.contains("認証完了"), "レスポンス: {response}");
    }

    #[test]
    fn test_parse_callback_from_request_line_empty_returns_error() {
        // parts.len() < 2 のケース
        let result = parse_callback_from_request_line("GET");
        assert!(matches!(result, Err(GcalError::AuthError(_))));
    }

    #[test]
    fn test_parse_callback_from_query_ignores_unknown_params() {
        // 不明なパラメータは無視され、code と state だけ取れること
        let cb = parse_callback_from_query("code=abc&state=xyz&extra=ignored").unwrap();
        assert_eq!(cb.code, "abc");
        assert_eq!(cb.state, "xyz");
    }

    #[test]
    fn test_url_decode_invalid_percent_encoding() {
        // 無効な16進数（%ZZ）はスキップされる
        let result = url_decode("val%ZZue");
        assert_eq!(result, "value");
    }
}
