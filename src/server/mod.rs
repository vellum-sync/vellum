use std::{
    env::current_exe,
    os::unix::process::CommandExt,
    path::Path,
    process::{self, Command, exit},
    sync::{Arc, Mutex},
    thread,
};

use chrono::{DateTime, Utc};
use clap;
use fork::{Fork, daemon};
use log::{debug, error, info};

use crate::{
    api::{Connection, Listener, Message},
    client,
    config::Config,
    error::Result,
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

pub fn run(config: &Config, args: Args) -> Result<()> {
    if args.stop {
        client::stop_server(config)
    } else if args.foreground {
        start(config)
    } else if let Fork::Child = daemon(false, false)? {
        background(config);
        exit(0);
    } else {
        exit(0);
    }
}

fn start(config: &Config) -> Result<()> {
    let pid = process::id();
    debug!("server: config={config:?} pid={pid}");

    let server = Server::new(config)?;

    server.serve()
}

fn background(config: &Config) {
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

#[derive(Debug)]
struct Entry {
    ts: DateTime<Utc>,
    host: String,
    cmd: String,
}

impl Entry {
    fn new(host: String, cmd: String) -> Self {
        let ts = Utc::now();
        Self { ts, host, cmd }
    }
}

#[derive(Debug)]
struct State {
    history: Vec<Entry>,
}

impl State {
    fn new() -> Self {
        Self {
            history: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct Server {
    cfg: Config,
    state: Arc<Mutex<State>>,
}

impl Server {
    fn new(cfg: &Config) -> Result<Self> {
        let pid = process::id();
        debug!("server: config={cfg:?} pid={pid}");

        Ok(Self {
            cfg: cfg.clone(),
            state: Arc::new(Mutex::new(State::new())),
        })
    }

    fn serve(&self) -> Result<()> {
        let listener = Listener::new(&self.cfg)?;
        for conn in listener.incoming() {
            match conn {
                Ok(conn) => {
                    let s = self.clone();
                    thread::spawn(move || s.handle_client(conn));
                }
                Err(e) => {
                    error!("Failed to accept connection: {e}");
                }
            }
        }

        Ok(())
    }

    fn handle_client(&self, mut conn: Connection) {
        loop {
            match conn.receive() {
                Ok(req) => {
                    info!("got request: {req:?}");
                    self.handle_request(req, &mut conn);
                }
                Err(e) => {
                    error!("error getting next request: {e}");
                    return;
                }
            }
        }
    }

    fn handle_request(&self, req: Message, conn: &mut Connection) {
        match req {
            Message::Store(cmd) => {
                info!("Recevied request to store command: {cmd}");
                self.store(cmd);
                if let Err(e) = conn.ack() {
                    error!("Failed to send ack: {e}");
                };
            }
            Message::HistoryRequest => {
                info!("Received history request");
                let history = self.history();
                if let Err(e) = conn.send_history(history) {
                    error!("Failed to send ack: {e}");
                };
            }
            Message::Exit => {
                info!("Received request to exit");
                if let Err(e) = conn.ack() {
                    error!("Failed to send ack: {e}");
                };
                if let Err(e) = Listener::remove_socket(&self.cfg) {
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

    fn store(&self, cmd: String) {
        let mut state = self.state.lock().unwrap();
        state.history.push(Entry::new(
            self.cfg.hostname.to_string_lossy().to_string(),
            cmd,
        ));
    }

    fn history(&self) -> Vec<String> {
        let state = self.state.lock().unwrap();
        state.history.iter().map(|e| e.cmd.clone()).collect()
    }
}
