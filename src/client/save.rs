use std::{
    fs::File,
    io::{BufRead, BufReader, Write, stdin, stdout},
};

use log::debug;

use clap::ValueHint;

use crate::{config::Config, error::Result, server};

#[derive(clap::Args, Debug)]
pub struct SaveArgs {
    /// Write to a file rather than stdout
    #[arg(short, long, value_hint = ValueHint::FilePath)]
    file: Option<String>,

    /// Save commands for all hosts, not just the current
    #[arg(short, long)]
    all_hosts: bool,
}

pub fn save(cfg: &Config, args: SaveArgs) -> Result<()> {
    let mut conn = server::ensure_ready(cfg)?;

    let mut history = conn.history_request()?;
    debug!("got history with {} entries", history.len());

    let writer: Box<dyn Write> = match args.file {
        Some(path) => Box::new(File::create(path)?),
        None => Box::new(stdout()),
    };

    if !args.all_hosts {
        let host = cfg.hostname.to_string_lossy().to_string();

        history.retain(|entry| entry.host == host);
    }

    serde_json::to_writer(writer, &history)?;

    Ok(())
}

#[derive(clap::Args, Debug)]
pub struct LoadArgs {
    /// Read from a file rather than stdin
    #[arg(short, long, value_hint = ValueHint::FilePath)]
    file: Option<String>,

    /// Load saved commands for all hosts, not just the current
    #[arg(short, long)]
    all_hosts: bool,
}

pub fn load(cfg: &Config, args: LoadArgs) -> Result<()> {
    let reader: Box<dyn BufRead> = match args.file {
        Some(path) => {
            let f = File::open(path)?;
            Box::new(BufReader::new(f))
        }
        None => Box::new(BufReader::new(stdin())),
    };

    let history = serde_json::from_reader(reader)?;

    let mut conn = server::ensure_ready(cfg)?;

    let count = conn.load(history, args.all_hosts)?;

    println!("Loaded {count} new/updated entries.");

    Ok(())
}
