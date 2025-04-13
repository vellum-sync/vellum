use std::{
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

    pub fn send(&self, msg: &Message) -> Result<()> {
        Ok(serde_json::to_writer(&self.s, msg)?)
    }

    pub fn receive(&self) -> Result<Message> {
        Ok(serde_json::from_reader(&self.s)?)
    }

    pub fn request(&self, msg: &Message) -> Result<Message> {
        debug!("send message: {msg:?}");
        self.send(msg)?;
        debug!("receive response");
        self.receive()
    }

    pub fn requests(&self) -> Requests<'_> {
        Requests { c: self }
    }

    pub fn store(&self, cmd: String) -> Result<()> {
        let msg = Message::Store(cmd);
        match self.request(&msg)? {
            Message::Ack => Ok(()),
            Message::Error(e) => Err(Error::Generic(e)),
            m => Err(Error::Generic(format!("unexpected response: {m:?}"))),
        }
    }

    pub fn history_request(&self) -> Result<Vec<String>> {
        let msg = Message::HistoryRequest;
        match self.request(&msg)? {
            Message::History(h) => Ok(h),
            Message::Error(e) => Err(Error::Generic(e)),
            m => Err(Error::Generic(format!("unexpected response: {m:?}"))),
        }
    }

    pub fn exit(&self) -> Result<()> {
        let msg = Message::Exit;
        match self.request(&msg)? {
            Message::Ack => Ok(()),
            Message::Error(e) => Err(Error::Generic(e)),
            m => Err(Error::Generic(format!("unexpected response: {m:?}"))),
        }
    }
}

pub struct Requests<'a> {
    c: &'a Connection,
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
