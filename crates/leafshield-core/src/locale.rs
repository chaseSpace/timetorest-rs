use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::OnceLock};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    #[default]
    #[serde(rename = "zh-CN")]
    Chinese,
    #[serde(rename = "en")]
    English,
}

pub fn tr(language: Language, text: &str) -> String {
    if language == Language::Chinese {
        return text.to_owned();
    }
    static MESSAGES: OnceLock<HashMap<String, String>> = OnceLock::new();
    MESSAGES
        .get_or_init(|| {
            serde_json::from_str(include_str!("../../../locales/en.json"))
                .expect("valid bundled translations")
        })
        .get(text)
        .cloned()
        .unwrap_or_else(|| text.to_owned())
}

pub fn phrases(language: Language) -> Vec<String> {
    crate::PHRASES
        .iter()
        .map(|phrase| tr(language, phrase))
        .collect()
}

pub fn phrase_index(phrase: &str) -> Option<usize> {
    crate::PHRASES
        .iter()
        .position(|preset| *preset == phrase || tr(Language::English, preset) == phrase)
}
