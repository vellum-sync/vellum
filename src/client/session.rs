use std::env;

use chrono::{DateTime, Utc};

use crate::{error::Result, history::Entry};

pub struct Session {
    pub id: String,
    pub start: Option<DateTime<Utc>>,
}

impl Session {
    pub fn get() -> Result<Self> {
        let id = match env::var("VELLUM_SESSION") {
            Ok(s) => s,
            Err(_) => "NO-SESSION".to_string(),
        };
        let start = match env::var("VELLUM_SESSION_START") {
            Ok(s) => Some(DateTime::parse_from_rfc3339(&s)?.to_utc()),
            Err(_) => None,
        };
        Ok(Self { id, start })
    }

    pub fn includes_entry(&self, entry: &Entry) -> bool {
        if let Some(start) = self.start {
            if entry.ts < start {
                return true;
            }
        };
        entry.session == self.id
    }
}
