use std::io::{Write, stdout};

use chrono::Utc;
use log::debug;
use uuid::Uuid;

use crate::{
    assets,
    error::{Error, Result},
    history::generate_key,
};

#[derive(clap::Args, Debug)]
pub struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, clap::Subcommand)]
enum Commands {
    /// Output a setup script for bash
    Bash,

    /// Output a setup script for zsh
    Zsh,

    /// Output an encryption key, suitable for use as $VELLUM_KEY
    Key,

    /// Output a session id, suitable for use as $VELLUM_SESSION
    Session,

    /// Output a timestamp, suitable for use as $VELLUM_SESSION_START
    Timestamp,
}

pub fn init(args: Args) -> Result<()> {
    match args.command {
        Commands::Bash => show_bash(),
        Commands::Zsh => show_zsh(),
        Commands::Key => show_key(),
        Commands::Session => show_session(),
        Commands::Timestamp => show_timestamp(),
    }
}

fn show_bash() -> Result<()> {
    debug!("init bash ...");
    let script = assets::get_file("init.bash")
        .ok_or_else(|| Error::Generic("bash init script missing".to_string()))?;
    stdout().write_all(script.contents())?;
    Ok(())
}

fn show_zsh() -> Result<()> {
    debug!("init zsh ...");
    let script = assets::get_file("init.zsh")
        .ok_or_else(|| Error::Generic("zsh init script missing".to_string()))?;
    stdout().write_all(script.contents())?;
    Ok(())
}

fn show_key() -> Result<()> {
    debug!("show key ...");
    print!("{}", generate_key()?);
    Ok(())
}

fn show_session() -> Result<()> {
    debug!("show session ...");
    print!("{}", Uuid::now_v7());
    Ok(())
}

fn show_timestamp() -> Result<()> {
    debug!("show timestamp ...");
    print!("{}", Utc::now().format("%+"));
    Ok(())
}
