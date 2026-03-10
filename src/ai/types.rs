use serde::{Deserialize, Serialize};

/// gcal events -p の第1段階: 操作種別とイベント特定ヒントを表す
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct AiOperationIntent {
    /// "add" | "update" | "delete"
    pub operation: String,
    /// update / delete 時のイベント特定ヒント（add 時は null）
    pub target: Option<AiEventTarget>,
}

/// update / delete 対象イベントの特定ヒント
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct AiEventTarget {
    /// タイトルのキーワード（部分一致検索に使用）
    pub title_hint: Option<String>,
    /// 日付ヒント（既存 parser で実日時範囲に変換）
    pub date_hint: Option<String>,
    /// カレンダーエイリアス（未指定なら CLI 引数 or デフォルト）
    pub calendar: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AiEventParameters {
    pub title: Option<String>,
    pub date: Option<String>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub location: Option<String>,
    #[serde(default)]
    pub repeat_rule: Option<String>, // e.g., "weekly", "monthly"
    #[serde(default)]
    pub reminder: Option<String>, // e.g., "popup:10m", "email:1h"
    #[serde(default)]
    pub calendar: Option<String>, // e.g., "仕事", "個人"（エイリアス名または ID）
}
