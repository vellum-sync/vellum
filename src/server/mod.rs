use std::{
    env::current_exe,
    os::unix::process::CommandExt,
    path::Path,
    process::{self, Command, exit},
    thread::{self, sleep},
    time::Duration,
};

use clap;
use fork::{Fork, daemon};
use log::{debug, error, info};

use crate::{
    api::{Connection, Server},
    client,
    config::Config,
    error::Error,
};

#[derive(clap::Args, Debug, Default)]
pub struct Args {
    /// Run the server in the foreground
    #[arg(short, long)]
    foreground: bool,

    /// Stop the current server, instead of starting a new one
    #[arg(short, long)]
    stop: bool,
}

pub fn run(config: &Config, args: Args) -> Result<(), Error> {
    if args.stop {
        client::stop_server(config)
    } else if args.foreground {
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

    let server = Server::new(config)?;

    for conn in server.incoming() {
        match conn {
            Ok(conn) => {
                let cfg = config.clone();
                thread::spawn(|| handle_client(conn, cfg));
            }
            Err(e) => {
                error!("Failed to accept connection: {e}");
            }
        }
    }

    sleep(Duration::from_secs(300));

    Ok(())
}

fn handle_client(mut conn: Connection, _config: Config) {
    for request in conn.requests() {
        match request {
            Ok(req) => {
                info!("got request: {req:?}");
            }
            Err(e) => {
                error!("error getting next request: {e}");
                return;
            }
        }
    }
}

fn background(config: &Config, _args: Args) {
    let log_file = Path::new(&config.state_dir).join("server.log");
    let exe = current_exe().expect("failed to get executable path");
    let mut cmd = Command::new(exe);
    if let Some(cfg) = config.path.as_ref() {
        cmd.arg("--config").arg(cfg);
    };
    let _ = cmd
        .args(["server", "--foreground"])
        .env("VELLUM_LOG_FILE", log_file)
        .exec();
}
