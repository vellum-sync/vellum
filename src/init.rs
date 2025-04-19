use std::io::{Write, stdout};

use clap;
use log::debug;

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
    /// Output a setup script for zsh
    ZSH,

    /// Output an encryption key, suitable for use as $VELLUM_KEY
    Key,
}

pub fn init(args: Args) -> Result<()> {
    match args.command {
        Commands::ZSH => show_zsh(),
        Commands::Key => show_key(),
    }
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
