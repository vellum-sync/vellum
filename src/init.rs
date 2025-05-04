use std::{
    fs::{self, File},
    io::{Write, stdout},
    path::Path,
};

use chrono::Utc;
use clap::{Command, ValueHint};
use clap_mangen::Man;
use flate2::{Compression, write::GzEncoder};
use log::{debug, info};
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

    /// Output the man page for vellum
    Man {
        /// Destination directory to write man pages to
        #[clap(value_hint = ValueHint::DirPath)]
        dest: String,
    },
}

pub fn init(args: Args, cmd: Command) -> Result<()> {
    match args.command {
        Commands::Bash => show_bash(),
        Commands::Zsh => show_zsh(),
        Commands::Key => show_key(),
        Commands::Session => show_session(),
        Commands::Timestamp => show_timestamp(),
        Commands::Man { dest } => show_manpage(dest, cmd),
    }
}

fn show_bash() -> Result<()> {
    debug!("init bash ...");
    let script =
        assets::get_file("init.bash").ok_or_else(|| Error::from_str("bash init script missing"))?;
    stdout().write_all(script.contents())?;
    Ok(())
}

fn show_zsh() -> Result<()> {
    debug!("init zsh ...");
    let script =
        assets::get_file("init.zsh").ok_or_else(|| Error::from_str("zsh init script missing"))?;
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

fn show_manpage(dest: String, cmd: Command) -> Result<()> {
    fs::create_dir_all(&dest)?;
    let dest = Path::new(&dest);
    write_manpage(dest, &[], &cmd)
}

fn write_manpage<P: AsRef<Path>>(dest: P, prefix: &[&str], cmd: &Command) -> Result<()> {
    let dest = dest.as_ref();

    let mut prefix = prefix.to_vec();
    let name = cmd.get_display_name().unwrap_or_else(|| cmd.get_name());
    prefix.push(name);

    for subcommand in cmd.get_subcommands() {
        write_manpage(dest, &prefix, subcommand)?;
    }

    let cmd = cmd
        .clone()
        .bin_name(prefix.join(" "))
        .display_name(prefix.join("-"));

    let page = Man::new(cmd);
    let filename = dest.join(page.get_filename() + ".gz");
    info!("Write {filename:?}");
    let mut out = GzEncoder::new(File::create(&filename)?, Compression::default());
    page.render(&mut out)?;

    Ok(())
}
