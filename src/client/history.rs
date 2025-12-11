use std::{cmp, collections::HashSet};

use log::debug;

use crate::{
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

    /// Show the history index numbers
    #[arg(short, long)]
    number: bool,

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

    /// Combine the history command with a cd to the recorded path
    #[arg(long)]
    cd: bool,

    /// Show the path where the command was run
    #[arg(short = 'p', long)]
    show_path: bool,

    /// Output the history information as JSON, instead of formatted for human
    /// reading.
    #[arg(short, long)]
    json: bool,

    /// Format the output in the way expected by fzf
    #[arg(long)]
    fzf: bool,

    /// The first entry in the history to show, negative values count back from
    /// the end (after filters have been applied).
    #[arg(default_value = "-10")]
    first: isize,

    /// The last entry in the history to show, negative values count back from
    /// the end (after filters have been applied). It has to be after the start
    /// point.
    #[arg(default_value = "-1")]
    last: isize,
}

impl HistoryArgs {
    fn get_cmd(&self, entry: &Entry) -> String {
        if self.cd && !entry.path.is_empty() {
            format!("cd \"{}\" && {}", entry.path, entry.cmd)
        } else {
            entry.cmd.clone()
        }
    }
}

pub fn history(cfg: &Config, args: HistoryArgs) -> Result<()> {
    if args.fzf {
        fzf_history(cfg, args)
    } else if args.json {
        json_history(cfg, args)
    } else {
        text_history(cfg, args)
    }
}

fn fzf_history(cfg: &Config, args: HistoryArgs) -> Result<()> {
    let filter = Filter::new(&args.filter)?;
    let mut conn = server::ensure_ready(cfg)?;

    let history = filter.enumerate_history_request(&mut conn)?;
    debug!("got filtered history with {} entries", history.len());

    let index_size = (history.len() + 1).to_string().len().next_multiple_of(8);

    let mut seen = HashSet::new();
    for (index, entry) in history
        .iter()
        .rev()
        .filter(|(_, entry)| seen.insert(&entry.cmd))
    {
        let cmd = args.get_cmd(entry);
        if args.show_path {
            print!("{:<index_size$} {}\t{}\x00", index + 1, entry.path, cmd);
        } else {
            print!("{}\t{}\x00", index + 1, cmd);
        }
    }

    Ok(())
}

fn json_history(cfg: &Config, args: HistoryArgs) -> Result<()> {
    let filter = Filter::new(args.filter)?;
    let mut conn = server::ensure_ready(cfg)?;

    let mut history = filter.history_request(&mut conn)?;
    debug!("got filtered history with {} entries", history.len());

    if args.reverse {
        history.reverse();
    }

    let json = serde_json::to_string(&history)?;
    println!("{json}");

    Ok(())
}

fn text_history(cfg: &Config, args: HistoryArgs) -> Result<()> {
    let filter = Filter::new(&args.filter)?;
    let mut conn = server::ensure_ready(cfg)?;

    let history = filter.enumerate_history_request(&mut conn)?;
    debug!("got filtered history with {} entries", history.len());

    let index_size = (history.len() + 1).to_string().len();
    let host_size = history
        .iter()
        .fold(0, |max, (_, entry)| cmp::max(max, entry.host.len()));
    let path_size = history
        .iter()
        .fold(0, |max, (_, entry)| cmp::max(max, entry.path.len()));

    if args.verbose && !args.no_headers {
        if args.id {
            println!(
                "{:36}\t{:host_size$}\t{:35}\t{:path_size$}\tCOMMAND",
                "ID", "HOST", "TIMESTAMP", "PATH"
            );
        } else {
            println!(
                "{:index_size$}\t{:host_size$}\t{:35}\t{:path_size$}\tCOMMAND",
                "INDEX", "HOST", "TIMESTAMP", "PATH"
            );
        }
    }

    let mut seen = HashSet::new();
    let mut filtered: Vec<&(usize, Entry)> = history
        .iter()
        .rev()
        .filter(|(_, entry)| !args.no_duplicates || seen.insert(&entry.cmd))
        .collect();
    if !args.reverse {
        filtered.reverse();
    }

    let first = get_index("FIRST", args.first, &filtered)?;
    let last = get_index("LAST", args.last, &filtered)?;
    debug!("show history from {first} to {last}");

    if !filtered.is_empty() && last < first {
        return Err(Error::Generic(format!(
            "LAST ({}) must not be before FIRST ({})",
            args.last, args.first
        )));
    }

    for (index, entry) in filtered {
        if index < &first || index > &last {
            continue;
        }
        if args.verbose {
            if args.id {
                println!(
                    "{:36}\t{:host_size$}\t{:35}\t{:path_size$}\t{}",
                    entry.id,
                    entry.host,
                    entry.ts.to_rfc3339(),
                    entry.path,
                    entry.cmd
                );
            } else {
                println!(
                    "{:index_size$}\t{:host_size$}\t{:35}\t{:path_size$}\t{}",
                    index + 1,
                    entry.host,
                    entry.ts.to_rfc3339(),
                    entry.path,
                    entry.cmd
                );
            }
            continue;
        }
        if args.number {
            print!("{:index_size$}\t", index + 1);
        }
        if args.id {
            print!("{:36}\t", entry.id);
        }
        if args.show_path {
            print!("{:path_size$}\t", entry.path);
        }
        println!("{}", args.get_cmd(entry));
    }

    Ok(())
}

fn get_index(label: &str, idx: isize, history: &[&(usize, Entry)]) -> Result<usize> {
    let max = history
        .iter()
        .map(|(index, _)| index)
        .next_back()
        .cloned()
        .unwrap_or(0);
    match idx {
        0 => Err(Error::Generic(format!("0 is not a valid {label} value"))),
        n if n < 0 && ((-n as usize) <= max) => index_of_last_n(-n as usize, history),
        n if n < 0 && ((-n as usize) > max) => Ok(0_usize),
        n if n > 0 && ((n as usize) <= max) => Ok((n - 1) as usize),
        n => Err(Error::Generic(format!(
            "Can't use {label} of {n} with {max} entries",
        ))),
    }
}

fn index_of_last_n(n: usize, history: &[&(usize, Entry)]) -> Result<usize> {
    debug!("get index of last {n} entries");
    let idx = history
        .iter()
        .rev()
        .take(n)
        .map(|(idx, _)| idx)
        .next_back()
        .cloned()
        .ok_or_else(|| {
            Error::Generic(format!(
                "Can't get last {n} of {} entry history",
                history.len()
            ))
        })?;
    debug!("got index: {idx}");
    Ok(idx)
}
