use std::{
    fs::remove_file,
    io::{Read, Write},
    os::unix::net::{self, UnixListener, UnixStream},
    path::Path,
    thread::sleep,
    time::{Duration, Instant},
};

use log::{debug, info};
use serde::{Deserialize, Serialize};

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

    pub fn receive(&mut self) -> Result<Message> {
        let mut buf = [0 as u8; 8];
        self.s.read_exact(&mut buf)?;
        let len = u64::from_le_bytes(buf);

        let mut data = vec![0u8; len as usize];
        self.s.read_exact(&mut data)?;

        Ok(rmp_serde::from_slice(&data)?)
    }

    pub fn request(&mut self, msg: &Message) -> Result<Message> {
        debug!("send message: {msg:?}");
        self.send(msg)?;
        debug!("receive response");
        self.receive()
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

impl<'a> Iterator for Incoming<'a> {
    type Item = Result<Connection>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.i.next() {
            Some(Ok(s)) => Some(Ok(Connection { s })),
            Some(Err(e)) => Some(Err(Error::IO(e))),
            None => None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    Ack,
    Store { cmd: String, session: String },
    Error(String),
    HistoryRequest,
    History(Vec<Entry>),
    Sync(bool),
    Exit(bool),
    Ping,
    Pong,
}

pub fn ping(cfg: &Config, wait: bool) -> Result<()> {
    let limit = Duration::from_secs(5);
    let start = Instant::now();
    while start.elapsed() < limit {
        match try_ping(cfg) {
            Ok(_) => return Ok(()),
            Err(e) => {
                if !wait {
                    return Err(e);
                }
            }
        }
        sleep(Duration::from_millis(100));
    }
    Err(Error::Generic(format!(
        "server didn't respond to ping within {limit:?}"
    )))
}

fn try_ping(cfg: &Config) -> Result<()> {
    let mut conn = Connection::new(cfg)?;
    conn.ping()?;
    Ok(())
}
