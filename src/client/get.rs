use uuid::Uuid;

use crate::{
    config::Config,
    error::{Error, Result},
    server,
};

#[derive(clap::Args, Debug)]
pub struct GetArgs {
    /// Get history entry by ID instead of index.
    #[arg(long)]
    id: bool,

    /// The history entry to get (index, or ID if --id given).
    entry: String,
}

pub fn get(cfg: &Config, args: GetArgs) -> Result<()> {
    let mut conn = server::ensure_ready(cfg)?;
    let history = conn.history_request()?;

    let entry = if args.id {
        let id = Uuid::parse_str(&args.entry)?;
        history
            .iter()
            .find(|entry| entry.id == id)
            .ok_or_else(|| Error::Generic(format!("history entry with ID {id} not found")))?
    } else {
        let idx = args.entry.parse::<usize>()?;
        history
            .get(idx)
            .ok_or_else(|| Error::Generic(format!("history entry with index {idx} not found")))?
    };

    println!("{}", entry.cmd);

    Ok(())
}
