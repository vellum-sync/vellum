use std::{
    fs::File,
    io::{BufRead, BufReader, stdin},
};

use clap::ValueHint;

use crate::{config::Config, error::Result, server};

use super::Session;

#[derive(clap::Args, Debug)]
pub struct ImportArgs {
    /// Read from a file rather than stdin
    #[arg(short, long, value_hint = ValueHint::FilePath)]
    file: Option<String>,

    /// Import into the current session, rather than marking as imported
    #[arg(long)]
    current_session: bool,
}

pub fn import(cfg: &Config, args: ImportArgs) -> Result<()> {
    let reader: Box<dyn BufRead> = match args.file {
        Some(path) => {
            let f = File::open(path)?;
            Box::new(BufReader::new(f))
        }
        None => Box::new(BufReader::new(stdin())),
    };

    let mut conn = server::ensure_ready(cfg)?;

    let session = if args.current_session {
        Session::get()?.id
    } else {
        "IMPORTED".to_string()
    };

    for line in reader.lines() {
        let line = line?;
        conn.store(line, "".to_string(), session.clone())?;
    }
    Ok(())
}
