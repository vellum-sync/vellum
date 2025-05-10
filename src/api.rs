use std::{
    fs::remove_file,
    io::{self, ErrorKind, Read, Write},
    os::unix::net::{self, UnixListener, UnixStream},
    path::Path,
    result,
    thread::sleep,
    time::{Duration, Instant},
};

use log::{debug, info};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    config::Config,
    error::{Error, Result},
    history::Entry,
};

pub struct Connection {
    s: UnixStream,
}

impl Connection {
    pub fn new(cfg: &Config) -> Result<Self> {
        let path = Path::new(&cfg.state_dir).join("server.sock");
        debug!("Connect to {path:#?}");
        let stream = UnixStream::connect(path)?;
        Ok(Connection { s: stream })
    }

    pub fn send(&mut self, msg: &Message) -> Result<()> {
        let data = rmp_serde::to_vec(msg)?;
        let len = data.len() as u64;
        self.s.write_all(&len.to_le_bytes())?;
        Ok(self.s.write_all(&data)?)
    }

    fn read_message(&mut self) -> result::Result<Vec<u8>, io::Error> {
        let mut buf = [0_u8; 8];
        self.s.read_exact(&mut buf)?;
        let len = u64::from_le_bytes(buf);

        let mut data = vec![0u8; len as usize];
        self.s.read_exact(&mut data)?;

        Ok(data)
    }

    pub fn receive(&mut self) -> Result<Option<Message>> {
        let data = match self.read_message() {
            Ok(d) => d,
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e.into()),
        };

        Ok(Some(rmp_serde::from_slice(&data)?))
    }

    pub fn request(&mut self, msg: &Message) -> Result<Message> {
        debug!("send message: {msg:?}");
        self.send(msg)?;
        debug!("receive response");
        let data = self.read_message()?;
        Ok(rmp_serde::from_slice(&data)?)
    }

    pub fn store(&mut self, cmd: String, session: String) -> Result<()> {
        let msg = Message::Store { cmd, session };
        match self.request(&msg)? {
            Message::Ack => Ok(()),
            Message::Error(e) => Err(Error::Generic(e)),
            m => Err(Error::Generic(format!("unexpected response: {m:?}"))),
        }
    }

    pub fn history_request(&mut self) -> Result<Vec<Entry>> {
        let msg = Message::HistoryRequest;
        match self.request(&msg)? {
            Message::History(h) => Ok(h),
            Message::Error(e) => Err(Error::Generic(e)),
            m => Err(Error::Generic(format!("unexpected response: {m:?}"))),
        }
    }

    pub fn send_history(&mut self, history: Vec<Entry>) -> Result<()> {
        let msg = Message::History(history);
        self.send(&msg)
    }

    pub fn sync(&mut self, force: bool) -> Result<()> {
        let msg = Message::Sync(force);
        match self.request(&msg)? {
            Message::Ack => Ok(()),
            Message::Error(e) => Err(Error::Generic(e)),
            m => Err(Error::Generic(format!("unexpected response: {m:?}"))),
        }
    }

    pub fn exit(&mut self, no_sync: bool) -> Result<()> {
        let msg = Message::Exit(no_sync);
        match self.request(&msg)? {
            Message::Ack => Ok(()),
            Message::Error(e) => Err(Error::Generic(e)),
            m => Err(Error::Generic(format!("unexpected response: {m:?}"))),
        }
    }

    pub fn ack(&mut self) -> Result<()> {
        let msg = Message::Ack;
        self.send(&msg)
    }

    pub fn error(&mut self, msg: String) -> Result<()> {
        let msg = Message::Error(msg);
        self.send(&msg)
    }

    pub fn ping(&mut self) -> Result<()> {
        let msg = Message::Ping;
        match self.request(&msg)? {
            Message::Pong => Ok(()),
            Message::Error(e) => Err(Error::Generic(e)),
            m => Err(Error::Generic(format!("unexpected response: {m:?}"))),
        }
    }

    pub fn pong(&mut self) -> Result<()> {
        let msg = Message::Pong;
        self.send(&msg)
    }

    pub fn update(&mut self, id: Uuid, cmd: String, session: String) -> Result<()> {
        let msg = Message::Update { id, cmd, session };
        match self.request(&msg)? {
            Message::Ack => Ok(()),
            Message::Error(e) => Err(Error::Generic(e)),
            m => Err(Error::Generic(format!("unexpected response: {m:?}"))),
        }
    }

    pub fn rebuild(&mut self) -> Result<Rebuilder<'_>> {
        let msg = Message::Rebuild;
        self.send(&msg)?;
        Ok(Rebuilder::new(self))
    }

    pub fn rebuild_status(&mut self, status: String) -> Result<()> {
        let msg = Message::RebuildStatus(status);
        self.send(&msg)
    }

    pub fn rebuild_complete(&mut self, result: Result<()>) -> Result<()> {
        let result = match result {
            Ok(()) => None,
            Err(e) => Some(format!("{e}")),
        };
        let msg = Message::RebuildComplete(result);
        self.send(&msg)
    }
}

#[derive(Debug)]
pub struct Listener {
    l: UnixListener,
}

impl Listener {
    pub fn new(cfg: &Config) -> Result<Self> {
        let path = Path::new(&cfg.state_dir).join("server.sock");
        debug!("Start listening: {path:#?}");
        let listener = UnixListener::bind(&path)?;
        info!("Started listening at {path:?}");
        Ok(Listener { l: listener })
    }

    pub fn incoming(&self) -> Incoming<'_> {
        Incoming {
            i: self.l.incoming(),
        }
    }

    pub fn remove_socket(cfg: &Config) -> Result<()> {
        let path = Path::new(&cfg.state_dir).join("server.sock");
        debug!("Removing socket {path:?}");
        Ok(remove_file(path)?)
    }
}

pub struct Incoming<'a> {
    i: net::Incoming<'a>,
}

impl Iterator for Incoming<'_> {
    type Item = Result<Connection>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.i.next() {
            Some(Ok(s)) => Some(Ok(Connection { s })),
            Some(Err(e)) => Some(Err(Error::IO(e))),
            None => None,
        }
    }
}

pub struct Rebuilder<'a> {
    conn: &'a mut Connection,
    complete: bool,
}

impl<'a> Rebuilder<'a> {
    fn new(conn: &'a mut Connection) -> Self {
        Self {
            conn,
            complete: false,
        }
    }
}

impl Iterator for Rebuilder<'_> {
    type Item = Result<String>;

    fn next(&mut self) -> Option<<Self as Iterator>::Item> {
        if self.complete {
            return None;
        }
        let msg = match self.conn.receive() {
            Ok(Some(msg)) => msg,
            Ok(None) => {
                self.complete = true;
                return Some(Err(Error::from_str("server disconnected!")));
            }
            Err(e) => {
                self.complete = true;
                return Some(Err(e));
            }
        };
        match msg {
            Message::RebuildStatus(status) => Some(Ok(status)),
            Message::RebuildComplete(result) => match result {
                Some(msg) => {
                    self.complete = true;
                    Some(Err(Error::Generic(format!("server returned error: {msg}"))))
                }
                None => None,
            },
            m => {
                self.complete = true;
                Some(Err(Error::Generic(format!("unexpected response: {m:?}"))))
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    Ack,
    Store {
        cmd: String,
        session: String,
    },
    Error(String),
    HistoryRequest,
    History(Vec<Entry>),
    Sync(bool),
    Exit(bool),
    Ping,
    Pong,
    Update {
        id: Uuid,
        cmd: String,
        session: String,
    },
    Rebuild,
    RebuildStatus(String),
    RebuildComplete(Option<String>),
}

pub fn ping(cfg: &Config, wait: Option<Duration>) -> Result<Connection> {
    let start = Instant::now();
    loop {
        match try_ping(cfg) {
            Ok(conn) => {
                debug!("took {:?} to get response from server", start.elapsed());
                return Ok(conn);
            }
            Err(e) => {
                if wait.is_none() {
                    return Err(e);
                }
            }
        }

        let limit = wait.unwrap();
        if start.elapsed() >= limit {
            return Err(Error::Generic(format!(
                "server didn't respond to ping within {limit:?}"
            )));
        }

        sleep(Duration::from_millis(100));
    }
}

fn try_ping(cfg: &Config) -> Result<Connection> {
    let mut conn = Connection::new(cfg)?;
    conn.ping()?;
    Ok(conn)
}
