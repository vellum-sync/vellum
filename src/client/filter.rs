use std::time::Duration;

use chrono::{DateTime, Utc};
use clap::ValueHint;

use crate::{api::Connection, error::Result, history::Entry};

use super::Session;

#[derive(clap::Args, Debug)]
pub struct FilterArgs {
    /// Only include commands stored by the current session
    #[arg(short, long)]
    session: bool,

    /// Only include commands stored on or after this time (RFC 3339 timestamp)
    #[arg(long, value_name = "TIMESTAMP", value_hint = ValueHint::Other)]
    after: Option<DateTime<Utc>>,

    /// Only include commands stored before this time (RFC 3339 timestamp)
    #[arg(long, value_name = "TIMESTAMP", value_hint = ValueHint::Other)]
    before: Option<DateTime<Utc>>,

    /// Only include commands stored by a specified host (can be specified
    /// multiple times)
    #[arg(long, value_hint = ValueHint::Hostname)]
    host: Option<Vec<String>>,

    /// Only include commands that were stored more than the given duration ago
    #[arg(long, value_parser = humantime::parse_duration, value_name = "DURATION", value_hint = ValueHint::Other)]
    min_age: Option<Duration>,

    /// Only include commands that were stored within the specified duration
    #[arg(long, value_parser = humantime::parse_duration, value_name = "DURATION", value_hint = ValueHint::Other)]
    max_age: Option<Duration>,

    /// Only include commands that match the given prefix
    #[arg(long, value_hint = ValueHint::Other)]
    prefix: Option<String>,

    /// Only include commands that include the given string
    #[arg(long, value_hint = ValueHint::Other)]
    search: Option<String>,
}

pub struct Filter {
    args: FilterArgs,

    min_age: Option<DateTime<Utc>>,
    max_age: Option<DateTime<Utc>>,
    current_session: Session,
}

impl Filter {
    pub fn new(args: FilterArgs) -> Result<Self> {
        let current_session = Session::get()?;
        let now = Utc::now();
        let min_age = args.min_age.map(|d| now - d);
        let max_age = args.max_age.map(|d| now - d);
        Ok(Self {
            args,
            min_age,
            max_age,
            current_session,
        })
    }

    pub fn entry(&self, entry: &Entry) -> bool {
        if self.args.session && !self.current_session.includes_entry(entry) {
            return false;
        }
        if let Some(after) = self.args.after {
            if entry.ts < after {
                return false;
            }
        }
        if let Some(before) = self.args.before {
            if entry.ts >= before {
                return false;
            }
        }
        if let Some(host) = &self.args.host {
            if !host.contains(&entry.host) {
                return false;
            }
        }
        if let Some(min_age) = self.min_age {
            if entry.ts >= min_age {
                return false;
            }
        }
        if let Some(max_age) = self.max_age {
            if entry.ts < max_age {
                return false;
            }
        }
        if let Some(prefix) = &self.args.prefix {
            if !entry.cmd.starts_with(prefix) {
                return false;
            }
        }
        if let Some(search) = &self.args.search {
            if !entry.cmd.contains(search) {
                return false;
            }
        }
        true
    }

    pub fn history_request(&self, conn: &mut Connection) -> Result<Vec<Entry>> {
        Ok(conn
            .history_request()?
            .into_iter()
            .filter(|entry| self.entry(entry))
            .collect())
    }
}
