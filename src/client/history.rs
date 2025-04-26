use std::{cmp, collections::HashSet};

use crate::{api::Connection, config::Config, error::Result, history::Entry, server};

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
}

pub fn history(cfg: &Config, args: HistoryArgs) -> Result<()> {
    server::ensure_ready(cfg)?;
    let filter = Filter::new(args.filter)?;
    let mut conn = Connection::new(cfg)?;
    let mut history: Vec<Entry> = conn
        .history_request()?
        .into_iter()
        .filter(|entry| filter.entry(entry))
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
        for (index, entry) in filtered {
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
