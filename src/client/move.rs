use log::debug;
use uuid::Uuid;

use crate::{api::Connection, config::Config, error::Result, history::Entry, server};

use super::Session;

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
