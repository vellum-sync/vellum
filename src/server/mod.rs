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
    api::{Connection, Message, Server},
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

fn handle_client(mut conn: Connection, config: Config) {
    loop {
        match conn.receive() {
            Ok(req) => {
                info!("got request: {req:?}");
                handle_request(req, &mut conn, &config);
            }
            Err(e) => {
                error!("error getting next request: {e}");
                return;
            }
        }
    }
}

fn handle_request(req: Message, conn: &mut Connection, cfg: &Config) {
    match req {
        Message::Store(cmd) => {
            info!("Recevied request to store command: {cmd}");
            if let Err(e) = conn.ack() {
                error!("Failed to send ack: {e}");
            };
        }
        Message::HistoryRequest => {
            info!("Received history request");
            let history = vec!["...".to_owned()];
            if let Err(e) = conn.send_history(history) {
                error!("Failed to send ack: {e}");
            };
        }
        Message::Exit => {
            info!("Received request to exit");
            if let Err(e) = conn.ack() {
                error!("Failed to send ack: {e}");
            };
            if let Err(e) = Server::remove_socket(cfg) {
                error!("Failed to remove server socket: {e}");
            }
            debug!("Exiting ...");
            exit(0);
        }
        r => {
            error!("received unknown request: {r:?}");
            if let Err(e) = conn.error(format!("unknown request: {r:?}")) {
                error!("Failed to send ack: {e}");
            };
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
