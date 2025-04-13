use std::{
    fs::remove_file,
    io::{Read, Write},
    os::unix::net::{self, UnixListener, UnixStream},
    path::Path,
};

use log::debug;
use serde::{Deserialize, Serialize};

use crate::{
    config::Config,
    error::{Error, Result},
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
        let data = serde_json::to_vec(msg)?;
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

        Ok(serde_json::from_slice(&data)?)
    }

    pub fn request(&mut self, msg: &Message) -> Result<Message> {
        debug!("send message: {msg:?}");
        self.send(msg)?;
        debug!("receive response");
        self.receive()
    }

    pub fn requests(&mut self) -> Requests<'_> {
        Requests { c: self }
    }

    pub fn store(&mut self, cmd: String) -> Result<()> {
        let msg = Message::Store(cmd);
        match self.request(&msg)? {
            Message::Ack => Ok(()),
            Message::Error(e) => Err(Error::Generic(e)),
            m => Err(Error::Generic(format!("unexpected response: {m:?}"))),
        }
    }

    pub fn history_request(&mut self) -> Result<Vec<String>> {
        let msg = Message::HistoryRequest;
        match self.request(&msg)? {
            Message::History(h) => Ok(h),
            Message::Error(e) => Err(Error::Generic(e)),
            m => Err(Error::Generic(format!("unexpected response: {m:?}"))),
        }
    }

    pub fn send_history(&mut self, history: Vec<String>) -> Result<()> {
        let msg = Message::History(history);
        self.send(&msg)
    }

    pub fn exit(&mut self) -> Result<()> {
        let msg = Message::Exit;
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
}

pub struct Requests<'a> {
    c: &'a mut Connection,
}

impl<'a> Iterator for Requests<'a> {
    type Item = Result<Message>;

    fn next(&mut self) -> Option<Self::Item> {
        Some(self.c.receive())
    }
}

pub struct Server {
    l: UnixListener,
}

impl Server {
    pub fn new(cfg: &Config) -> Result<Self> {
        let path = Path::new(&cfg.state_dir).join("server.sock");
        debug!("Start listening: {path:#?}");
        let listener = UnixListener::bind(path)?;
        Ok(Server { l: listener })
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
    Store(String),
    Error(String),
    HistoryRequest,
    History(Vec<String>),
    Exit,
}
