use std::io::{Write, stdout};

use clap;
use log::debug;

use crate::{
    assets,
    error::{Error, Result},
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
}

pub fn init(args: Args) -> Result<()> {
    match args.command {
        Commands::ZSH => show_zsh(),
    }
}

fn show_zsh() -> Result<()> {
    debug!("init zsh ...");
    let script = assets::get_file("init.zsh")
        .ok_or_else(|| Error::Generic("zsh init script missing".to_string()))?;
    stdout().write_all(script.contents())?;
    Ok(())
}
