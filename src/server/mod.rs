use std::{
    env::current_exe,
    fmt::Display,
    os::unix::process::CommandExt,
    path::Path,
    process::{self, Command, exit},
    thread::sleep,
    time::Duration,
};

use clap;
use fork::{Fork, daemon};
use log::debug;

use crate::config::Config;

pub enum Error {
    Daemon(i32),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Daemon(errno) => write!(f, "failed to start server daemon (errno={errno})"),
        }
    }
}

impl From<i32> for Error {
    fn from(value: i32) -> Self {
        Self::Daemon(value)
    }
}

#[derive(clap::Args, Debug, Default)]
pub struct Args {
    /// Run the server in the foreground
    #[arg(short, long)]
    foreground: bool,
}

pub fn run(config: &Config, args: Args) -> Result<(), Error> {
    if args.foreground {
        start(config, args)
    } else if let Fork::Child = daemon(false, false)? {
        background(config, args);
        exit(0);
    } else {
        exit(0);
    }
}

fn start(config: &Config, args: Args) -> Result<(), Error> {
    let pid = process::id();
    debug!("server: config={config:?} args={args:?} pid={pid}");

    sleep(Duration::from_secs(300));

    Ok(())
}

fn background(config: &Config, _args: Args) {
    let log_file = Path::new(&config.state_dir).join("server.log");
    let exe = current_exe().expect("failed to get executable path");
    let mut cmd = Command::new(exe);
    if let Some(cfg) = config.path.as_ref() {
        cmd.args(["--config", cfg]);
    };
    let _ = cmd
        .args(["server", "--foreground"])
        .env("VELLUM_LOG_FILE", log_file)
        .exec();
}
