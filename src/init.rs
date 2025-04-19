use std::io::{Write, stdout};

use clap;
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
    BASH,

    /// Output a setup script for zsh
    ZSH,

    /// Output an encryption key, suitable for use as $VELLUM_KEY
    Key,

    /// Output a session id, suitable for use as $VELLUM_SESSION
    Session,
}

pub fn init(args: Args) -> Result<()> {
    match args.command {
        Commands::BASH => show_bash(),
        Commands::ZSH => show_zsh(),
        Commands::Key => show_key(),
        Commands::Session => show_session(),
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
