use std::{
    env::{self, current_exe},
    fs::{self, File},
    io::Write,
    os::unix::process::CommandExt,
    path::Path,
    process::{self, Command, exit},
    sync::{
        Arc, Mutex,
        atomic::AtomicBool,
        mpsc::{SyncSender, sync_channel},
    },
    thread,
    time::Duration,
};

use chrono::{DurationRound, TimeDelta, Utc};
use clap::{self, crate_version};
use fd_lock::RwLock;
use fork::{Fork, daemon};
use humantime::format_duration;
use log::{debug, error, info};
use signal_hook::{consts::TERM_SIGNALS, flag, iterator::Signals};
use uuid::Uuid;

use crate::{
    api::{Connection, Listener, Message, ping},
    client,
    config::Config,
    error::{Error, Result},
    history::{self, Entry, History},
    process::server_is_running,
    sync::{Syncer, get_syncer},
};

#[derive(clap::Args, Debug, Default)]
pub struct Args {
    /// Run the server in the foreground
    #[arg(short, long)]
    foreground: bool,

    /// Wait for the server to start
    #[arg(short, long)]
    wait: bool,

    /// Try to start the server, even if one appears to be running
    #[arg(long)]
    force: bool,

    /// Stop the existing server, if there is one
    #[arg(short, long)]
    restart: bool,
}

pub fn run(config: &Config, args: Args) -> Result<()> {
    // make sure that we have a crypt key before trying to run a server,
    // otherwise things aren't going to go very well ...
    if let Err(e) = history::get_key() {
        error!("Unable to get crypt key from $VELLUM_KEY, refusing to start server:");
        error!("  {e}");
        exit(1);
    }

    if args.restart {
        client::stop_server(config, false)?;
    }

    if !args.force && server_is_running(config)? {
        error!("Server is already running!");
        exit(1);
    }

    if args.foreground {
        start(config)
    } else if args.wait {
        debug!("start the server");
        ensure_running(config, true)?;
        debug!("wait for server to respond ...");
        ping(config, true)?;
        return Ok(());
    } else if let Fork::Child = daemon(false, false)? {
        background(config, args.force);
        exit(0);
    } else {
        exit(0);
    }
}

fn start(config: &Config) -> Result<()> {
    let pid = process::id();
    debug!("server: config={config:?} pid={pid}");

    // try and lock the pid file, this will fail if a server is already running.
    // We can't truncate as part of opening the file as this will bypass the
    // lock and remove the pid of another server - so we have to manually
    // truncate after opening to ensure that writing a smaller value than was
    // previously written works correctly.
    debug!("create pid file");
    let pid_file = Path::new(&config.state_dir).join("server.pid");
    let mut f = RwLock::new(File::options().create(true).write(true).open(pid_file)?);
    let mut pid_lock = f.try_write()?;
    pid_lock.set_len(0)?;
    write!(pid_lock, "{}", pid)?;
    pid_lock.flush()?;

    info!("Starting vellum server v{} (pid: {pid})", crate_version!());

    // clean up an old socket file if there is one. We should only get here if
    // we got the pid lock.
    debug!("check for old server socket");
    let server_sock = Path::new(&config.state_dir).join("server.sock");
    if fs::exists(&server_sock)? {
        debug!("remove old server socket");
        fs::remove_file(&server_sock)?;
    }

    debug!("create server");
    let server = Server::new(config)?;

    debug!("start server");
    server.serve()?;

    drop(pid_lock);

    Ok(())
}

fn background(config: &Config, force: bool) {
    let log_file = Path::new(&config.state_dir).join("server.log");
    let exe = current_exe().expect("failed to get executable path");
    let mut cmd = Command::new(exe);
    if let Some(cfg) = config.path.as_ref() {
        cmd.arg("--config").arg(cfg);
    };
    if let Ok(value) = env::var("VELLUM_SERVER_LOG") {
        cmd.env("VELLUM_LOG", value);
    };
    cmd.args(["server", "--foreground"]);
    if force {
        cmd.arg("--force");
    }
    let _ = cmd.env("VELLUM_LOG_FILE", log_file).exec();
}

fn ensure_running(cfg: &Config, force: bool) -> Result<()> {
    if !force && server_is_running(cfg)? {
        debug!("server is already running");
        return Ok(());
    };

    debug!("start server in background");
    let exe = current_exe()?;
    let mut cmd = Command::new(exe);
    if let Some(cfg_path) = cfg.path.as_ref() {
        cmd.arg("--config").arg(cfg_path);
    };
    cmd.arg("server");
    if force {
        cmd.arg("--force");
    }
    cmd.spawn()?;

    Ok(())
}

pub fn ensure_ready(cfg: &Config) -> Result<()> {
    ensure_running(cfg, false)?;
    debug!("wait for server to respond ...");
    ping(cfg, true)?;
    debug!("server is ready");
    Ok(())
}

#[derive(Debug, Clone)]
struct Server {
    cfg: Config,
    host: String,
    // NOTE: syncer should always be locked before history.
    syncer: Arc<Mutex<Box<dyn Syncer>>>,
    history: Arc<Mutex<History>>,
}

impl Server {
    fn new(cfg: &Config) -> Result<Self> {
        let pid = process::id();
        debug!("server: config={cfg:?} pid={pid}");

        let host = cfg.hostname.to_string_lossy().to_string();
        let syncer = get_syncer(cfg)?;

        let path = syncer.refresh()?;

        let s = Self {
            cfg: cfg.clone(),
            history: Arc::new(Mutex::new(History::load(host.clone(), path)?)),
            host,
            syncer: Arc::new(Mutex::new(syncer)),
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
        let interval = match TimeDelta::from_std(self.cfg.sync.interval) {
            Ok(i) => i,
            Err(e) => {
                error!("failed to convert sync interval: {e}");
                exit(1)
            }
        };
        loop {
            let next = match Utc::now().duration_round_up(interval) {
                Ok(n) => n,
                Err(e) => {
                    error!("failed to calculate next sync interval: {e}");
                    exit(1)
                }
            };
            debug!("next sync is: {next}");
            let wait = match (next - Utc::now()).to_std() {
                Ok(w) => w,
                Err(e) => {
                    error!("failed to calculate wait: {e}");
                    exit(1)
                }
            };
            debug!("wait is: {}", format_duration(wait));
            thread::sleep(wait);
            if let Err(e) = self.sync(false) {
                error!("Failed to run background sync: {e}");
            }
        }
    }

    fn serve(&self) -> Result<()> {
        self.setup_signals()?;

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

    fn setup_signals(&self) -> Result<()> {
        let term_now = Arc::new(AtomicBool::new(false));
        // Getting two term signals in a row will trigger immediate exit
        for sig in TERM_SIGNALS {
            flag::register_conditional_shutdown(*sig, 1, term_now.clone())?;
            flag::register(*sig, term_now.clone())?;
        }
        let mut signals = Signals::new(TERM_SIGNALS)?;
        let server = self.clone();
        thread::spawn(move || {
            for signal in signals.forever() {
                info!("Received signal: {signal}");
                // run a sync before exiting, so that we don't loose any state.
                if let Err(e) = server.sync_local(false) {
                    error!("Failed to sync: {e}");
                }
                info!("Exiting ...");
                exit(0);
            }
        });
        Ok(())
    }

    fn handle_client(&self, mut conn: Connection) {
        loop {
            match conn.receive() {
                Ok(Some(req)) => {
                    debug!("got request: {req:?}");
                    self.handle_request(req, &mut conn);
                }
                Ok(None) => {
                    debug!("client disconnected");
                    return;
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
            Message::Store { cmd, session } => {
                debug!("Received request from session {session} to store command: {cmd}");
                self.store(cmd, session);
                if let Err(e) = conn.ack() {
                    error!("Failed to send ack: {e}");
                };
            }
            Message::HistoryRequest => {
                debug!("Received history request");
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
                info!("Exiting ...");
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
            Message::Ping => {
                debug!("Received ping request");
                if let Err(e) = conn.pong() {
                    error!("Failed to send pong: {e}");
                }
            }
            Message::Update { id, cmd, session } => {
                debug!("Received request from session {session} to update command {id}: {cmd}");
                if let Err(e) = self.update(id, cmd, session) {
                    error!("Failed to update {id}: {e}");
                    if let Err(e) = conn.error(format!("{e}")) {
                        error!("Failed to send error: {e}");
                    }
                }
                if let Err(e) = conn.ack() {
                    error!("Failed to send ack: {e}");
                };
            }
            Message::Rebuild => {
                debug!("Received request to rebuild data store");
                let s = self.clone();
                let (sender, receiver) = sync_channel(0);
                let worker = thread::spawn(move || s.rebuild(sender));
                for status in receiver {
                    if let Err(e) = conn.rebuild_status(status) {
                        error!("Failed to send status: {e}");
                    }
                }
                let result = match worker.join() {
                    Ok(r) => r,
                    Err(e) => Err(Error::Generic(format!("rebuild thread paniced: {e:?}"))),
                };
                if let Err(e) = conn.rebuild_complete(result) {
                    error!("Failed to send complete: {e}");
                }
            }
            r => {
                error!("received unknown request: {r:?}");
                if let Err(e) = conn.error(format!("unknown request: {r:?}")) {
                    error!("Failed to send ack: {e}");
                };
            }
        }
    }

    fn store(&self, cmd: String, session: String) {
        let mut history = self.history.lock().unwrap();
        history.add(cmd, session);
    }

    fn history(&self) -> Vec<Entry> {
        let history = self.history.lock().unwrap();
        history.history()
    }

    fn sync_local(&self, force: bool) -> Result<()> {
        let syncer = self.syncer.lock().unwrap();
        let path = syncer.refresh()?;
        {
            // we want to lock the history for the shortest time that we can
            let mut history = self.history.lock().unwrap();
            history.save(path)?;
        }
        syncer.push_changes(&self.host, force)
    }

    fn sync(&self, force: bool) -> Result<()> {
        let syncer = self.syncer.lock().unwrap();
        let path = syncer.refresh()?;
        {
            // we want to lock the history for the shortest time that we can
            let mut history = self.history.lock().unwrap();
            history.sync(path)?;
        }
        syncer.push_changes(&self.host, force)
    }

    fn update(&self, id: Uuid, cmd: String, session: String) -> Result<()> {
        let mut history = self.history.lock().unwrap();
        history.update(id, cmd, session)
    }

    fn rebuild(&self, sender: SyncSender<String>) -> Result<()> {
        debug!("rebuild background thread started");

        sender.send("Refreshing git state".to_string())?;
        let syncer = self.syncer.lock().unwrap();
        let path = syncer.refresh()?;
        let history = self.history.lock().unwrap();

        sender.send("Locking git repo ...".to_string())?;
        let sync_lock = syncer.lock()?;

        sender.send("Waiting 5s to allow in progress syncs to complete ...".to_string())?;
        thread::sleep(Duration::from_secs(5));

        sender.send("Unlocking git repo ...".to_string())?;
        sync_lock.unlock()?;

        // we need to drop the lock first, otherwise we can't drop syncer
        drop(sync_lock);
        // make sure that syncer and history stay around for the whole function
        // so that they stay locked for the entire time.
        drop(history);
        drop(syncer);

        debug!("rebuild background thread complete");
        Ok(())
    }
}
