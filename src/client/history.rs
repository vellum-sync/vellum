use std::{cmp, collections::HashSet};

use log::debug;

use crate::{
    api::Connection,
    config::Config,
    error::{Error, Result},
    history::Entry,
    server,
};

use super::{Filter, FilterArgs};

#[derive(clap::Args, Debug)]
pub struct HistoryArgs {
    #[command(flatten)]
    filter: FilterArgs,

    /// Show the command IDs (unique IDs that can be used for modifying history)
    #[arg(long)]
    id: bool,

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

    /// The first entry in the history to show, negative values count back from
    /// the end.
    first: Option<isize>,

    /// The last entry in the history to show, negative values count back from
    /// the end. It has to be after the start point.
    last: Option<isize>,
}

pub fn history(cfg: &Config, args: HistoryArgs) -> Result<()> {
    server::ensure_ready(cfg)?;
    let filter = Filter::new(args.filter)?;
    let mut conn = Connection::new(cfg)?;
    let mut history: Vec<Entry> = filter.history_request(&mut conn)?;
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
            if args.id {
                println!(
                    "{:36}\t{:host_size$}\t{:35}\tCOMMAND",
                    "ID", "HOST", "TIMESTAMP"
                );
            } else {
                println!(
                    "{:index_size$}\t{:host_size$}\t{:35}\tCOMMAND",
                    "INDEX", "HOST", "TIMESTAMP"
                );
            }
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
        let first = get_index("FIRST", args.first, filtered.len())?.unwrap_or(0);
        let last = get_index("LAST", args.last, filtered.len())?.unwrap_or(filtered.len() - 1);
        debug!("show history from {first} to {last}");
        if !filtered.is_empty() && last < first {
            return Err(Error::Generic(format!(
                "LAST ({}) must not be before FIRST ({})",
                args.last.unwrap(),
                args.first.unwrap()
            )));
        }
        for (index, entry) in filtered {
            if index < first || index > last {
                continue;
            }
            if args.verbose {
                if args.id {
                    println!(
                        "{:36}\t{:host_size$}\t{:35}\t{}",
                        entry.id,
                        entry.host,
                        entry.ts.to_rfc3339(),
                        entry.cmd
                    );
                } else {
                    println!(
                        "{:index_size$}\t{:host_size$}\t{:35}\t{}",
                        index + 1,
                        entry.host,
                        entry.ts.to_rfc3339(),
                        entry.cmd
                    );
                }
            } else if args.id {
                println!("{}\t{}", entry.id, entry.cmd);
            } else {
                println!("{}", entry.cmd);
            }
        }
    }
    Ok(())
}

fn get_index(label: &str, idx: Option<isize>, max: usize) -> Result<Option<usize>> {
    match idx {
        Some(0) => Err(Error::Generic(format!("0 is not a valid {label} value"))),
        Some(n) if n < 0 && (((-n) as usize) <= max) => Ok(Some((max as isize + n) as usize)),
        Some(n) if n > 0 && ((n as usize) <= max) => Ok(Some((n - 1) as usize)),
        Some(n) => Err(Error::Generic(format!(
            "Can't use {label} of {n} with {max} entries",
        ))),
        None => Ok(None),
    }
}
