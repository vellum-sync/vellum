use std::{collections::HashSet, env};

use log::debug;
use uuid::Uuid;

use crate::{api::Connection, config::Config, error::Result, history::Entry};

fn get_session() -> String {
    match env::var("VELLUM_SESSION") {
        Ok(s) => s,
        Err(_) => "NO-SESSION".to_string(),
    }
}

pub fn store(cfg: &Config, cmd: String) -> Result<()> {
    let mut conn = Connection::new(cfg)?;
    conn.store(cmd, get_session())
}

pub fn stop_server(cfg: &Config, no_sync: bool) -> Result<()> {
    let mut conn = Connection::new(cfg)?;
    conn.exit(no_sync)
}

#[derive(clap::Args, Debug)]
pub struct HistoryArgs {
    /// Only show commands stored by the current session
    #[arg(short, long)]
    session: bool,

    /// Format the output in the way expected by fzf
    #[arg(long)]
    fzf: bool,
}

pub fn history(cfg: &Config, args: HistoryArgs) -> Result<()> {
    let current_session = get_session();
    let mut conn = Connection::new(cfg)?;
    let history: Vec<Entry> = conn
        .history_request()?
        .into_iter()
        .filter(|entry| !args.session || entry.session == current_session)
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
    } else {
        for entry in history {
            println!("{}", entry.cmd);
        }
    }
    Ok(())
}

pub fn sync(cfg: &Config, force: bool) -> Result<()> {
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

    /// How far to move through the history relative to the start
    distance: isize,

    /// Where to start the movement in the history. By default movement will
    /// start from after the most recent command (i.e. by default a distance of
    /// -1 will move back to the most recent command).
    start: Option<String>,
}

pub fn do_move(cfg: &Config, args: MoveArgs) -> Result<()> {
    debug!("move: {args:?}");

    let mut conn = Connection::new(cfg)?;
    let current_session = get_session();
    let history: Vec<Entry> = conn
        .history_request()?
        .into_iter()
        .filter(|entry| !args.session || entry.session == current_session)
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
