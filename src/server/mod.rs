use std::{
    cmp::Ordering,
    collections::HashMap,
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
use serde::{Deserialize, Serialize};

use crate::{
    api::{Connection, Listener, Message},
    client,
    config::Config,
    error::Result,
    sync::{Syncer, Version, get_syncer},
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

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
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

impl Ord for Entry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.ts
            .cmp(&other.ts)
            .then(self.host.cmp(&other.host))
            .then(self.cmd.cmp(&other.cmd))
    }
}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug)]
struct ExternalHistory {
    latest: Version,
    history: Vec<Entry>,
}

impl ExternalHistory {
    fn version(&self, force: bool) -> Option<&Version> {
        if force { None } else { Some(&self.latest) }
    }
}

#[derive(Debug)]
struct State {
    host: String,
    changed: bool,
    history: Vec<Entry>,
    external: HashMap<String, ExternalHistory>,
    syncer: Box<dyn Syncer>,
}

impl State {
    fn new(host: String, syncer: Box<dyn Syncer>) -> Result<Self> {
        let mut s = Self {
            host,
            changed: false,
            history: Vec::new(),
            external: HashMap::new(),
            syncer,
        };
        s.load()?;
        Ok(s)
    }

    fn load(&mut self) -> Result<()> {
        if let Some(data) = self.syncer.get_newer(&self.host, None)? {
            self.history = serde_json::from_slice(&data.data)?;
        }
        // TODO(jp3): load the external data from the Syncer too
        Ok(())
    }

    fn combined_history(&self) -> Vec<Entry> {
        let mut combined = self.history.clone();
        for (_, external) in self.external.iter() {
            combined.extend_from_slice(&external.history);
        }
        combined.sort_unstable();
        combined
    }

    fn store(&mut self, host: String, cmd: String) {
        self.history.push(Entry::new(host, cmd));
        self.changed = true
    }

    fn sync(&mut self, force: bool) -> Result<()> {
        if self.changed || force {
            let data = serde_json::to_vec(&self.history)?;
            self.syncer.store(&self.host, &data)?;
        }
        for (host, external) in self.external.iter_mut() {
            if let Some(data) = self.syncer.get_newer(&host, external.version(force))? {
                let history: Vec<Entry> = serde_json::from_slice(&data.data)?;
                external.latest = data.version;
                external.history = history;
            }
        }
        // TODO(jp3): How do we learn about new hosts, or ones that have gone?
        Ok(())
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

        let host = cfg.hostname.to_string_lossy().to_string();
        let syncer = get_syncer(cfg)?;

        Ok(Self {
            cfg: cfg.clone(),
            state: Arc::new(Mutex::new(State::new(host, syncer)?)),
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
                    error!("Failed to send history: {e}");
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
            Message::Sync(force) => {
                info!("Received request to sync");
                if let Err(e) = self.sync(force) {
                    error!("Failed to sync: {e}");
                    if let Err(e) = conn.error(format!("failed to sync: {e}")) {
                        error!("Failed to send error: {e}");
                    }
                } else if let Err(e) = conn.ack() {
                    error!("Failed to send ack: {e}");
                };
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
        state.store(self.cfg.hostname.to_string_lossy().to_string(), cmd);
    }

    fn history(&self) -> Vec<String> {
        let state = self.state.lock().unwrap();
        state
            .combined_history()
            .iter()
            .map(|e| e.cmd.clone())
            .collect()
    }

    fn sync(&self, force: bool) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        state.sync(force)
    }
}
