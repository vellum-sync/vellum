use chrono::{DateTime, Utc};

use crate::{error::Result, history::Entry};

use super::Session;

#[derive(clap::Args, Debug)]
pub struct FilterArgs {
    /// Only include commands stored by the current session
    #[arg(short, long)]
    session: bool,

    /// Only include commands stored on or after this time (UTC ISO8601 timestamp)
    #[arg(long, value_name = "DATE")]
    after: Option<DateTime<Utc>>,

    /// Only include commands stored before this time (UTC ISO8601 timestamp)
    #[arg(long, value_name = "DATE")]
    before: Option<DateTime<Utc>>,

    /// Only include commands stored by a specified host (can be specified
    /// multiple times)
    #[arg(long)]
    host: Option<Vec<String>>,
}

pub struct Filter {
    args: FilterArgs,

    current_session: Session,
}

impl Filter {
    pub fn new(args: FilterArgs) -> Result<Self> {
        let current_session = Session::get()?;
        Ok(Self {
            args,
            current_session,
        })
    }

    pub fn entry(&self, entry: &Entry) -> bool {
        if self.args.session {
            if !self.current_session.includes_entry(entry) {
                return false;
            }
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
        true
    }
}
