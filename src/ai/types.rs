use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
