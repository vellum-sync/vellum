use std::{
    env::{self, current_exe},
    os::unix::process::CommandExt,
    path::Path,
    process::{self, Command, exit},
    sync::{Arc, Mutex},
    thread,
};

use clap;
use fork::{Fork, daemon};
use log::{debug, error, info};
use ticker::Ticker;

use crate::{
    api::{Connection, Listener, Message},
    config::Config,
    error::Result,
    history::{self, Entry, History},
    sync::get_syncer,
};

#[derive(clap::Args, Debug, Default)]
pub struct Args {
    /// Run the server in the foreground
    #[arg(short, long)]
    foreground: bool,
}

pub fn run(config: &Config, args: Args) -> Result<()> {
    // make sure that we have a crypt key before trying to run a server,
    // otherwise things aren't going to go very well ...
    if let Err(e) = history::get_key() {
        error!("Unable to get crypt key from $VELLUM_KEY, refusing to start server:");
        error!("  {e}");
        exit(1);
    }

    if args.foreground {
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
    if let Ok(value) = env::var("VELLUM_SERVER_LOG") {
        cmd.env("VELLUM_LOG", value);
    };
    let _ = cmd
        .args(["server", "--foreground"])
        .env("VELLUM_LOG_FILE", log_file)
        .exec();
}

#[derive(Debug, Clone)]
struct Server {
    cfg: Config,
    history: Arc<Mutex<History>>,
}

impl Server {
    fn new(cfg: &Config) -> Result<Self> {
        let pid = process::id();
        debug!("server: config={cfg:?} pid={pid}");

        let host = cfg.hostname.to_string_lossy().to_string();
        let syncer = get_syncer(cfg)?;

        let s = Self {
            cfg: cfg.clone(),
            history: Arc::new(Mutex::new(History::load(host, syncer)?)),
        };
        s.start_background_sync();

        Ok(s)
    }

    fn start_background_sync(&self) {
        if self.cfg.sync.interval.is_zero() {
            // don't start background sync if interval is zero
            return;
        }
        let s = self.clone();
        thread::spawn(move || s.background_sync());
    }

    fn background_sync(&self) {
        debug!(
            "starting background sync with {:?} interval",
            self.cfg.sync.interval
        );
        let ticker = Ticker::new(0.., self.cfg.sync.interval);
        for _ in ticker {
            if let Err(e) = self.sync(false) {
                error!("Failed to run background sync: {e}");
            }
        }
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
            Message::Exit(no_sync) => {
                info!("Received request to exit");
                if let Err(e) = conn.ack() {
                    error!("Failed to send ack: {e}");
                };
                if let Err(e) = Listener::remove_socket(&self.cfg) {
                    error!("Failed to remove server socket: {e}");
                }
                if !no_sync {
                    debug!("Run a final sync_local before exit");
                    // run a sync before exiting, so that we don't loose any state.
                    if let Err(e) = self.sync_local(false) {
                        error!("Failed to sync: {e}");
                    }
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
        let mut history = self.history.lock().unwrap();
        history.add(cmd);
    }

    fn history(&self) -> Vec<Entry> {
        let history = self.history.lock().unwrap();
        history.history()
    }

    fn sync_local(&self, force: bool) -> Result<()> {
        let mut history = self.history.lock().unwrap();
        history.save(force)
    }

    fn sync(&self, force: bool) -> Result<()> {
        let mut history = self.history.lock().unwrap();
        history.sync(force)
    }
}
