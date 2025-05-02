use std::collections::HashSet;

use clap::ValueHint;
use log::debug;
use uuid::Uuid;

use crate::{api::Connection, config::Config, error::Result, history::Entry, server};

use super::{Filter, FilterArgs};

#[derive(clap::Args, Debug)]
pub struct MoveArgs {
    #[command(flatten)]
    filter: FilterArgs,

    /// Include the history entry ID in the output
    #[arg(short, long)]
    with_id: bool,

    /// Only show the most recent version of each command in the history
    #[arg(short = 'D', long)]
    no_duplicates: bool,

    /// How far to move through the history relative to the start
    #[clap(value_hint = ValueHint::Other)]
    distance: isize,

    /// An ID of a historical command from which to start the movement in the
    /// history. By default movement will start from after the most recent
    /// command (i.e. by default a distance of -1 will move back to the most
    /// recent command).
    #[clap(value_hint = ValueHint::Other)]
    start: Option<String>,
}

pub fn do_move(cfg: &Config, args: MoveArgs) -> Result<()> {
    server::ensure_ready(cfg)?;

    debug!("move: {args:?}");

    let mut conn = Connection::new(cfg)?;
    let filter = Filter::new(args.filter)?;
    let mut history: Vec<Entry> = filter.history_request(&mut conn)?;

    if args.no_duplicates {
        history = remove_duplicates(history);
    }

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

fn remove_duplicates(history: Vec<Entry>) -> Vec<Entry> {
    let mut seen = HashSet::new();
    let mut filtered: Vec<Entry> = history
        .into_iter()
        .rev()
        .filter(|entry| seen.insert(entry.cmd.clone()))
        .collect();
    filtered.reverse();
    filtered
}
