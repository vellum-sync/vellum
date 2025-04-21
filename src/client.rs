use std::{cmp, collections::HashSet, env};

use chrono::{DateTime, Utc};
use log::{debug, info};
use uuid::Uuid;

use crate::{
    api::{self, Connection},
    config::Config,
    error::Result,
    history::Entry,
    process::{server_is_running, wait_for_server_exit},
    server,
};

struct Session {
    id: String,
    start: Option<DateTime<Utc>>,
}

impl Session {
    fn get() -> Result<Self> {
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

    fn includes_entry(&self, entry: &Entry) -> bool {
        if let Some(start) = self.start {
            if entry.ts < start {
                return true;
            }
        };
        entry.session == self.id
    }
}

pub fn store(cfg: &Config, cmd: String) -> Result<()> {
    server::ensure_ready(cfg)?;
    let mut conn = Connection::new(cfg)?;
    conn.store(cmd, Session::get()?.id)
}

pub fn stop_server(cfg: &Config, no_sync: bool) -> Result<()> {
    if !server_is_running(cfg)? {
        debug!("server isn't running");
        return Ok(());
    }
    debug!("server is running");
    let mut conn = Connection::new(cfg)?;
    conn.exit(no_sync)?;
    debug!("wait for server exit");
    wait_for_server_exit(cfg)
}

#[derive(clap::Args, Debug)]
pub struct HistoryArgs {
    /// Only show commands stored by the current session
    #[arg(short, long)]
    session: bool,

    /// Show more complete output, instead of just the commands
    #[arg(short, long)]
    verbose: bool,

    /// Don't include the headers in verbose output
    #[arg(short = 'H', long)]
    no_headers: bool,

    /// Only show the most recent version of each command in the history
    #[arg(short = 'D', long)]
    no_duplicates: bool,

    /// Output the most recent command first instead of last
    #[arg(short, long)]
    reverse: bool,

    /// Output the history information as JSON, instead of formatted for human
    /// reading.
    #[arg(short, long)]
    json: bool,

    /// Format the output in the way expected by fzf
    #[arg(long)]
    fzf: bool,
}

pub fn history(cfg: &Config, args: HistoryArgs) -> Result<()> {
    server::ensure_ready(cfg)?;
    let current_session = Session::get()?;
    let mut conn = Connection::new(cfg)?;
    let mut history: Vec<Entry> = conn
        .history_request()?
        .into_iter()
        .filter(|entry| !args.session || current_session.includes_entry(entry))
        .collect();
    let mut seen = HashSet::new();
    if args.fzf {
        for (index, entry) in history
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, entry)| seen.insert(&entry.cmd))
        {
            print!("{}\t{}\x00", index + 1, entry.cmd);
        }
    } else if args.json {
        if args.reverse {
            history.reverse();
        }
        let json = serde_json::to_string(&history)?;
        println!("{json}");
    } else {
        let index_size = (history.len() + 1).to_string().len();
        let host_size = history
            .iter()
            .fold(0, |max, entry| cmp::max(max, entry.host.len()));
        if args.verbose && !args.no_headers {
            println!(
                "{:index_size$}\t{:host_size$}\t{:35}\tCOMMAND",
                "INDEX", "HOST", "TIMESTAMP"
            );
        }
        let mut filtered: Vec<(usize, &Entry)> = history
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, entry)| !args.no_duplicates || seen.insert(&entry.cmd))
            .collect();
        if !args.reverse {
            filtered.reverse();
        }
        for (index, entry) in filtered {
            if args.verbose {
                println!(
                    "{:index_size$}\t{:host_size$}\t{:35}\t{}",
                    index + 1,
                    entry.host,
                    entry.ts.to_rfc3339(),
                    entry.cmd
                );
            } else {
                println!("{}", entry.cmd);
            }
        }
    }
    Ok(())
}

pub fn sync(cfg: &Config, force: bool) -> Result<()> {
    server::ensure_ready(cfg)?;
    let mut conn = Connection::new(cfg)?;
    conn.sync(force)
}

#[derive(clap::Args, Debug)]
pub struct MoveArgs {
    /// Include the history entry ID in the output
    #[arg(short, long)]
    with_id: bool,

    /// Only consider the history stored by the current session
    #[arg(short, long)]
    session: bool,

    /// Look for commands that match a given prefix
    #[arg(short, long)]
    prefix: Option<String>,

    /// How far to move through the history relative to the start
    distance: isize,

    /// An ID of a historical command from which to start the movement in the
    /// history. By default movement will start from after the most recent
    /// command (i.e. by default a distance of -1 will move back to the most
    /// recent command).
    start: Option<String>,
}

pub fn do_move(cfg: &Config, args: MoveArgs) -> Result<()> {
    server::ensure_ready(cfg)?;

    debug!("move: {args:?}");

    let mut conn = Connection::new(cfg)?;
    let current_session = Session::get()?;
    let history: Vec<Entry> = conn
        .history_request()?
        .into_iter()
        .filter(|entry| !args.session || current_session.includes_entry(entry))
        .filter(|entry| {
            args.prefix.is_none() || entry.cmd.starts_with(args.prefix.as_ref().unwrap())
        })
        .collect();

    let start = match args.start {
        Some(s) if !s.is_empty() => {
            let id = Uuid::parse_str(&s)?;
            history
                .iter()
                .position(|entry| entry.id == id)
                .unwrap_or_else(|| history.len())
        }
        _ => history.len(),
    };

    let want = start.saturating_add_signed(args.distance);
    debug!(
        "history has {} entries, start at {}, move by {}, so we want {}",
        history.len(),
        start,
        args.distance,
        want,
    );

    if want >= history.len() {
        println!("");
        return Ok(());
    }

    let entry = &history[want];
    if args.with_id {
        print!("{}|", entry.id);
    }
    println!("{}", entry.cmd);

    Ok(())
}

pub fn ping(cfg: &Config, wait: bool) -> Result<()> {
    api::ping(cfg, wait)?;
    info!("got pong from server");
    Ok(())
}
