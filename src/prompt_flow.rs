use std::io::Write;

use chrono::{DateTime, NaiveDate, Utc};

use crate::ai::client::AiClient;
use crate::ai::types::AiEventTarget;
use crate::cli_mapper::{
    naive_date_to_utc_end, naive_date_to_utc_start, AddCommandInput, CliMapper,
    UpdateCommandInput,
};
use crate::config::Config;
use crate::domain::{EventQuery, EventStart, EventSummary};
use crate::error::GcalError;
use crate::event_selector;
use crate::output::{write_events, write_new_event_dry_run, write_update_event_dry_run};
use crate::parser::parse_date_expr;
use crate::ports::CalendarClient;
use chrono::{Duration, Local, Timelike};

/// events -p / delete -p の共通 "操作種別判定 → イベント特定" ワークフロー。
/// `main.rs` は依存の組み立てだけを担い、ロジック本体はここに集約する。
///
/// `events -p <prompt>` のディスパッチフロー。
/// - add → parse_prompt → handle_add_event
/// - delete → find event → handle_delete_event
/// - update → find event → parse_prompt → handle_update_event
pub async fn dispatch_prompt_events<CAL, AI, W>(
    client: &CAL,
    ai: &AI,
    config: &Config,
    today: NaiveDate,
    prompt_str: &str,
    yes: bool,
    out: &mut W,
) -> Result<(), GcalError>
where
    CAL: CalendarClient,
    AI: AiClient,
    W: Write,
{
    let intent = ai.parse_operation_intent(prompt_str).await?;

    match intent.operation.as_str() {
        "add" => {
            let ai_params = ai.parse_prompt(prompt_str).await?;
            let calendar_id = ai_params
                .calendar
                .clone()
                .unwrap_or_else(|| "primary".to_string());
            let event = CliMapper::map_add_command(AddCommandInput {
                calendar: calendar_id.clone(),
                calendar_display_name: calendar_id,
                today,
                ai_params: Some(ai_params),
                ..Default::default()
            })?;
            if !yes {
                write_new_event_dry_run(&event, out)?;
            }
            let summary = event.summary.clone();
            let id = client.create_event(event).await?;
            writeln!(out, "作成しました: {} (ID: {})", summary, id)?;
        }

        "delete" => {
            let target = intent.target.unwrap_or(AiEventTarget {
                title_hint: None,
                date_hint: None,
                calendar: None,
            });
            let calendar_ids = config.resolve_event_calendars(None, None);
            let (time_min, time_max) = search_range(target.date_hint.as_deref(), today)?;
            let all_events = fetch_events(client, &calendar_ids, time_min, time_max).await?;
            let summaries: Vec<EventSummary> = all_events.iter().map(|(_, e)| e.clone()).collect();
            let matched = event_selector::filter_by_target(&summaries, &target, today);

            if matched.is_empty() {
                writeln!(out, "候補イベントが見つかりませんでした")?;
                return Ok(());
            }
            // yes=true / 単一候補のみ想定（テスト用。選択 UI は main.rs 側で行う）
            if !yes && matched.len() > 1 {
                return Err(GcalError::ConfigError(
                    "複数の候補があります。--yes を使用するか、より具体的な条件を指定してください"
                        .to_string(),
                ));
            }
            let (cal_id, event) = &all_events[matched[0]];
            client.delete_event(cal_id, &event.id).await?;
            writeln!(out, "削除しました (ID: {})", event.id)?;
        }

        "update" => {
            let target = intent.target.unwrap_or(AiEventTarget {
                title_hint: None,
                date_hint: None,
                calendar: None,
            });
            let calendar_ids = config.resolve_event_calendars(None, None);
            let (time_min, time_max) = search_range(target.date_hint.as_deref(), today)?;
            let all_events = fetch_events(client, &calendar_ids, time_min, time_max).await?;
            let summaries: Vec<EventSummary> = all_events.iter().map(|(_, e)| e.clone()).collect();
            let matched = event_selector::filter_by_target(&summaries, &target, today);

            if matched.is_empty() {
                writeln!(out, "候補イベントが見つかりませんでした")?;
                return Ok(());
            }
            if !yes && matched.len() > 1 {
                return Err(GcalError::ConfigError(
                    "複数の候補があります。--yes を使用するか、より具体的な条件を指定してください"
                        .to_string(),
                ));
            }
            let (cal_id, selected) = &all_events[matched[0]];
            let ai_params = ai.parse_prompt(prompt_str).await?;
            if !yes {
                let update_event_preview = CliMapper::map_update_command(UpdateCommandInput {
                    event_id: selected.id.clone(),
                    calendar: cal_id.clone(),
                    calendar_display_name: cal_id.clone(),
                    today,
                    ai_params: Some(ai_params.clone()),
                    ..Default::default()
                })?;
                write_update_event_dry_run(&update_event_preview, out)?;
            }
            let update_event = CliMapper::map_update_command(UpdateCommandInput {
                event_id: selected.id.clone(),
                calendar: cal_id.clone(),
                calendar_display_name: cal_id.clone(),
                today,
                ai_params: Some(ai_params),
                ..Default::default()
            })?;
            client.update_event(update_event).await?;
            writeln!(out, "更新しました (ID: {})", selected.id)?;
        }

        "show" => {
            let target = intent.target.unwrap_or(AiEventTarget {
                title_hint: None,
                date_hint: None,
                calendar: None,
            });
            let calendar_ids = config.resolve_event_calendars(None, None);
            let (time_min, time_max) = search_range(target.date_hint.as_deref(), today)?;
            let mut all_events: Vec<EventSummary> =
                fetch_events(client, &calendar_ids, time_min, time_max)
                    .await?
                    .into_iter()
                    .map(|(_, e)| e)
                    .collect();
            // title_hint がある場合はさらに絞り込む
            if target.title_hint.is_some() {
                let matched = event_selector::filter_by_target(&all_events, &target, today);
                all_events = matched.into_iter().map(|i| all_events[i].clone()).collect();
            }
            all_events.sort_by_key(|e| match &e.start {
                EventStart::Date(d) => (*d, 0u8, 0u32),
                EventStart::DateTime(dt) => {
                    let local = dt.with_timezone(&Local);
                    (local.date_naive(), 1, local.time().num_seconds_from_midnight())
                }
            });
            write_events(out, &all_events, false)?;
        }

        other => {
            return Err(GcalError::ConfigError(format!(
                "不明な操作種別: '{}' (add/update/delete/show のいずれかが必要)",
                other
            )));
        }
    }
    Ok(())
}

/// `delete -p` のワークフロー。
pub async fn dispatch_prompt_delete<CAL, AI, W>(
    client: &CAL,
    ai: &AI,
    config: &Config,
    today: NaiveDate,
    prompt_str: &str,
    force: bool,
    out: &mut W,
) -> Result<(), GcalError>
where
    CAL: CalendarClient,
    AI: AiClient,
    W: Write,
{
    let intent = ai.parse_operation_intent(prompt_str).await?;
    let target = intent.target.unwrap_or(AiEventTarget {
        title_hint: None,
        date_hint: None,
        calendar: None,
    });

    let calendar_ids = config.resolve_event_calendars(None, None);
    let (time_min, time_max) = search_range(target.date_hint.as_deref(), today)?;
    let all_events = fetch_events(client, &calendar_ids, time_min, time_max).await?;
    let summaries: Vec<EventSummary> = all_events.iter().map(|(_, e)| e.clone()).collect();
    let matched = event_selector::filter_by_target(&summaries, &target, today);

    if matched.is_empty() {
        writeln!(out, "候補イベントが見つかりませんでした")?;
        return Ok(());
    }
    if !force && matched.len() > 1 {
        return Err(GcalError::ConfigError(
            "複数の候補があります。--force を使用するか、より具体的な条件を指定してください"
                .to_string(),
        ));
    }
    let (cal_id, event) = &all_events[matched[0]];
    client.delete_event(cal_id, &event.id).await?;
    writeln!(out, "削除しました (ID: {})", event.id)?;
    Ok(())
}

/// date_hint から検索時間範囲を計算する。
pub fn search_range(
    date_hint: Option<&str>,
    today: NaiveDate,
) -> Result<(DateTime<Utc>, DateTime<Utc>), GcalError> {
    if let Some(hint) = date_hint {
        let range = parse_date_expr(hint, today)?;
        Ok((naive_date_to_utc_start(range.from)?, naive_date_to_utc_end(range.to)?))
    } else {
        Ok((
            naive_date_to_utc_start(today)?,
            naive_date_to_utc_end(today + Duration::days(14))?,
        ))
    }
}

/// 複数カレンダーからイベントを収集して (calendar_id, EventSummary) 形式で返す。
pub async fn fetch_events<CAL: CalendarClient>(
    client: &CAL,
    calendar_ids: &[String],
    time_min: DateTime<Utc>,
    time_max: DateTime<Utc>,
) -> Result<Vec<(String, EventSummary)>, GcalError> {
    let mut all_events = Vec::new();
    for cal_id in calendar_ids {
        let query = EventQuery { calendar_id: cal_id.clone(), time_min, time_max };
        let events = client.list_events(query).await?;
        for e in events {
            all_events.push((cal_id.clone(), e));
        }
    }
    Ok(all_events)
}

/// 候補イベントを番号付きで表示する（main.rs の選択 UI 用ヘルパー）。
pub fn format_candidate_list(events: &[(String, EventSummary)], matched: &[usize]) -> String {
    let mut result = "複数のイベントが見つかりました:\n".to_string();
    for (num, &idx) in matched.iter().enumerate() {
        let (_, e) = &events[idx];
        let date_str = match &e.start {
            EventStart::Date(d) => d.format("%Y/%m/%d").to_string(),
            EventStart::DateTime(dt) => {
                dt.with_timezone(&Local).format("%Y/%m/%d %H:%M").to_string()
            }
        };
        result.push_str(&format!("  {}. {} {}\n", num + 1, date_str, e.summary));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::{NaiveDate, TimeZone, Utc};
    use std::sync::{Arc, Mutex};

    use crate::ai::types::{AiEventParameters, AiEventTarget, AiOperationIntent};
    use crate::config::Config;
    use crate::domain::{CalendarSummary, EventQuery, EventStart, EventSummary, NewEvent, UpdateEvent};
    use crate::error::GcalError;
    use crate::ports::CalendarClient;

    // --- スタブ AI クライアント ---

    struct StubAiClient {
        intent: AiOperationIntent,
        params: AiEventParameters,
    }

    #[async_trait]
    impl AiClient for StubAiClient {
        async fn parse_prompt(&self, _: &str) -> Result<AiEventParameters, GcalError> {
            Ok(self.params.clone())
        }
        async fn parse_operation_intent(&self, _: &str) -> Result<AiOperationIntent, GcalError> {
            Ok(self.intent.clone())
        }
    }

    // --- フェイク カレンダークライアント ---

    struct FakeCalendarClient {
        events: Vec<EventSummary>,
        deleted_ids: Arc<Mutex<Vec<(String, String)>>>,
        updated_events: Arc<Mutex<Vec<UpdateEvent>>>,
        created_events: Arc<Mutex<Vec<NewEvent>>>,
    }

    impl FakeCalendarClient {
        fn new(events: Vec<EventSummary>) -> Self {
            Self {
                events,
                deleted_ids: Arc::new(Mutex::new(vec![])),
                updated_events: Arc::new(Mutex::new(vec![])),
                created_events: Arc::new(Mutex::new(vec![])),
            }
        }
    }

    #[async_trait]
    impl CalendarClient for FakeCalendarClient {
        async fn list_calendars(&self) -> Result<Vec<CalendarSummary>, GcalError> { Ok(vec![]) }
        async fn list_events(&self, _: EventQuery) -> Result<Vec<EventSummary>, GcalError> {
            Ok(self.events.clone())
        }
        async fn create_event(&self, event: NewEvent) -> Result<String, GcalError> {
            let id = format!("new-{}", event.summary);
            self.created_events.lock().unwrap().push(event);
            Ok(id)
        }
        async fn update_event(&self, event: UpdateEvent) -> Result<(), GcalError> {
            self.updated_events.lock().unwrap().push(event);
            Ok(())
        }
        async fn delete_event(&self, calendar_id: &str, event_id: &str) -> Result<(), GcalError> {
            self.deleted_ids.lock().unwrap().push((calendar_id.to_string(), event_id.to_string()));
            Ok(())
        }
    }

    fn today() -> NaiveDate { NaiveDate::from_ymd_opt(2026, 3, 10).unwrap() }

    fn make_event(id: &str, summary: &str, date: NaiveDate) -> EventSummary {
        EventSummary {
            id: id.to_string(),
            summary: summary.to_string(),
            start: EventStart::Date(date),
            end: None,
            location: None,
        }
    }

    fn delete_intent(title: &str, date: &str) -> AiOperationIntent {
        AiOperationIntent {
            operation: "delete".to_string(),
            target: Some(AiEventTarget {
                title_hint: Some(title.to_string()),
                date_hint: Some(date.to_string()),
                calendar: None,
            }),
        }
    }

    fn add_intent() -> AiOperationIntent {
        AiOperationIntent { operation: "add".to_string(), target: None }
    }

    fn update_intent(title: &str, date: &str) -> AiOperationIntent {
        AiOperationIntent {
            operation: "update".to_string(),
            target: Some(AiEventTarget {
                title_hint: Some(title.to_string()),
                date_hint: Some(date.to_string()),
                calendar: None,
            }),
        }
    }

    fn config_with_primary() -> Config {
        let mut c = Config::default();
        c.calendars = std::collections::HashMap::new();
        c
    }

    // --- dispatch_prompt_delete のテスト ---

    #[tokio::test]
    async fn test_dispatch_prompt_delete_single_match_deletes_event() {
        let event_date = NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
        let fake = FakeCalendarClient::new(vec![
            make_event("evt-001", "定例MTG", event_date),
        ]);
        let deleted = fake.deleted_ids.clone();
        let ai = StubAiClient {
            intent: delete_intent("定例MTG", "2026/3/10"),
            params: AiEventParameters::default(),
        };

        let mut out = Vec::new();
        dispatch_prompt_delete(
            &fake, &ai, &config_with_primary(), today(),
            "定例MTGを削除して", true, &mut out,
        ).await.unwrap();

        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("削除しました"), "output: {output}");

        let ids = deleted.lock().unwrap();
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0].1, "evt-001");
    }

    #[tokio::test]
    async fn test_dispatch_prompt_delete_no_match_prints_message() {
        let event_date = NaiveDate::from_ymd_opt(2026, 3, 11).unwrap();
        let fake = FakeCalendarClient::new(vec![
            make_event("evt-001", "ランチ", event_date),
        ]);
        let deleted = fake.deleted_ids.clone();
        let ai = StubAiClient {
            intent: delete_intent("定例MTG", "2026/3/10"),
            params: AiEventParameters::default(),
        };

        let mut out = Vec::new();
        dispatch_prompt_delete(
            &fake, &ai, &config_with_primary(), today(),
            "定例MTGを削除して", true, &mut out,
        ).await.unwrap();

        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("候補イベントが見つかりませんでした"), "output: {output}");
        assert!(deleted.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_dispatch_prompt_delete_multiple_matches_returns_error_without_force() {
        let d = NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
        let fake = FakeCalendarClient::new(vec![
            make_event("evt-001", "定例MTG", d),
            make_event("evt-002", "定例MTG", d),
        ]);
        let ai = StubAiClient {
            intent: delete_intent("定例MTG", "2026/3/10"),
            params: AiEventParameters::default(),
        };

        let mut out = Vec::new();
        let result = dispatch_prompt_delete(
            &fake, &ai, &config_with_primary(), today(),
            "定例MTGを削除して", false, &mut out,
        ).await;

        assert!(result.is_err());
    }

    // --- dispatch_prompt_events のテスト ---

    #[tokio::test]
    async fn test_dispatch_prompt_events_add_creates_event() {
        let fake = FakeCalendarClient::new(vec![]);
        let created = fake.created_events.clone();
        let ai = StubAiClient {
            intent: add_intent(),
            params: AiEventParameters {
                title: Some("新規会議".to_string()),
                date: Some("2026/3/11".to_string()),
                start: Some("14:00".to_string()),
                end: Some("15:00".to_string()),
                ..Default::default()
            },
        };

        let mut out = Vec::new();
        dispatch_prompt_events(
            &fake, &ai, &config_with_primary(), today(),
            "明日14時から新規会議を追加して", true, &mut out,
        ).await.unwrap();

        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("作成しました"), "output: {output}");

        let events = created.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary, "新規会議");
    }

    #[tokio::test]
    async fn test_dispatch_prompt_events_delete_removes_event() {
        let d = NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
        let fake = FakeCalendarClient::new(vec![
            make_event("evt-del", "朝会", d),
        ]);
        let deleted = fake.deleted_ids.clone();
        let ai = StubAiClient {
            intent: delete_intent("朝会", "2026/3/10"),
            params: AiEventParameters::default(),
        };

        let mut out = Vec::new();
        dispatch_prompt_events(
            &fake, &ai, &config_with_primary(), today(),
            "今日の朝会を削除して", true, &mut out,
        ).await.unwrap();

        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("削除しました"), "output: {output}");

        let ids = deleted.lock().unwrap();
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0].1, "evt-del");
    }

    #[tokio::test]
    async fn test_dispatch_prompt_events_update_modifies_event() {
        let d = NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
        let fake = FakeCalendarClient::new(vec![
            make_event("evt-upd", "朝会", d),
        ]);
        let updated = fake.updated_events.clone();
        let ai = StubAiClient {
            intent: update_intent("朝会", "2026/3/10"),
            params: AiEventParameters {
                title: Some("朝会（変更済み）".to_string()),
                date: Some("2026/3/10".to_string()),
                start: Some("10:00".to_string()),
                end: Some("10:30".to_string()),
                ..Default::default()
            },
        };

        let mut out = Vec::new();
        dispatch_prompt_events(
            &fake, &ai, &config_with_primary(), today(),
            "今日の朝会を10時に変更して", true, &mut out,
        ).await.unwrap();

        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("更新しました"), "output: {output}");

        let events = updated.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_id, "evt-upd");
    }

    #[tokio::test]
    async fn test_dispatch_prompt_events_unknown_operation_returns_error() {
        let fake = FakeCalendarClient::new(vec![]);
        let ai = StubAiClient {
            intent: AiOperationIntent { operation: "unknown".to_string(), target: None },
            params: AiEventParameters::default(),
        };

        let mut out = Vec::new();
        let result = dispatch_prompt_events(
            &fake, &ai, &config_with_primary(), today(),
            "??", true, &mut out,
        ).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_dispatch_prompt_events_no_match_prints_message() {
        let d = NaiveDate::from_ymd_opt(2026, 3, 11).unwrap();
        let fake = FakeCalendarClient::new(vec![
            make_event("evt-1", "ランチ", d),
        ]);
        let ai = StubAiClient {
            intent: delete_intent("存在しない会議", "2026/3/10"),
            params: AiEventParameters::default(),
        };

        let mut out = Vec::new();
        dispatch_prompt_events(
            &fake, &ai, &config_with_primary(), today(),
            "...", true, &mut out,
        ).await.unwrap();

        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("候補イベントが見つかりませんでした"), "output: {output}");
    }

    // --- fetch_events のテスト ---

    #[tokio::test]
    async fn test_fetch_events_collects_with_calendar_id() {
        let fake = FakeCalendarClient::new(vec![
            make_event("e1", "朝会", NaiveDate::from_ymd_opt(2026, 3, 10).unwrap()),
        ]);
        let time_min = Utc.with_ymd_and_hms(2026, 3, 10, 0, 0, 0).unwrap();
        let time_max = Utc.with_ymd_and_hms(2026, 3, 11, 0, 0, 0).unwrap();

        let result = fetch_events(&fake, &["primary".to_string()], time_min, time_max).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "primary");
        assert_eq!(result[0].1.id, "e1");
    }

    // --- show 操作のテスト ---

    fn show_intent(date: &str) -> AiOperationIntent {
        AiOperationIntent {
            operation: "show".to_string(),
            target: Some(AiEventTarget {
                title_hint: None,
                date_hint: Some(date.to_string()),
                calendar: None,
            }),
        }
    }

    #[tokio::test]
    async fn test_dispatch_prompt_events_show_displays_events() {
        let d = NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
        let fake = FakeCalendarClient::new(vec![
            make_event("e1", "朝会", d),
            make_event("e2", "ランチ", d),
        ]);
        let ai = StubAiClient {
            intent: show_intent("2026/3/10"),
            params: AiEventParameters::default(),
        };

        let mut out = Vec::new();
        dispatch_prompt_events(
            &fake, &ai, &config_with_primary(), today(),
            "今日の予定を見せて", true, &mut out,
        ).await.unwrap();

        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("朝会"), "output: {output}");
        assert!(output.contains("ランチ"), "output: {output}");
    }

    #[tokio::test]
    async fn test_dispatch_prompt_events_show_no_events_shows_empty_message() {
        let fake = FakeCalendarClient::new(vec![]);
        let ai = StubAiClient {
            intent: show_intent("2026/3/10"),
            params: AiEventParameters::default(),
        };

        let mut out = Vec::new();
        dispatch_prompt_events(
            &fake, &ai, &config_with_primary(), today(),
            "今日の予定を見せて", true, &mut out,
        ).await.unwrap();

        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("イベントが見つかりません"), "output: {output}");
    }

    #[tokio::test]
    async fn test_dispatch_prompt_events_show_with_title_hint_filters() {
        let d = NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
        let fake = FakeCalendarClient::new(vec![
            make_event("e1", "定例MTG", d),
            make_event("e2", "ランチ", d),
        ]);
        let ai = StubAiClient {
            intent: AiOperationIntent {
                operation: "show".to_string(),
                target: Some(AiEventTarget {
                    title_hint: Some("MTG".to_string()),
                    date_hint: Some("2026/3/10".to_string()),
                    calendar: None,
                }),
            },
            params: AiEventParameters::default(),
        };

        let mut out = Vec::new();
        dispatch_prompt_events(
            &fake, &ai, &config_with_primary(), today(),
            "今日のMTGを見せて", true, &mut out,
        ).await.unwrap();

        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("定例MTG"), "output: {output}");
        assert!(!output.contains("ランチ"), "ランチが表示されるべきでない: {output}");
    }
}
