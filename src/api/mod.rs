use std::{
    os::unix::net::{self, UnixListener, UnixStream},
    path::Path,
};

use log::debug;
use serde::{Deserialize, Serialize};

use crate::{config::Config, error::Error};

pub struct Connection {
    s: UnixStream,
}

impl Connection {
    pub fn new(cfg: &Config) -> Result<Self, Error> {
        let path = Path::new(&cfg.state_dir).join("server.sock");
        debug!("Connect to {path:#?}");
        let stream = UnixStream::connect(path)?;
        Ok(Connection { s: stream })
    }

    pub fn send(&self, msg: &Message) -> Result<(), Error> {
        Ok(serde_json::to_writer(&self.s, msg)?)
    }

    pub fn receive(&self) -> Result<Message, Error> {
        Ok(serde_json::from_reader(&self.s)?)
    }

    pub fn request(&self, msg: &Message) -> Result<Message, Error> {
        self.send(msg)?;
        self.receive()
    }

    pub fn requests(&self) -> Requests<'_> {
        Requests { c: self }
    }
}

pub struct Requests<'a> {
    c: &'a Connection,
}

impl<'a> Iterator for Requests<'a> {
    type Item = Result<Message, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        Some(self.c.receive())
    }
}

pub struct Server {
    l: UnixListener,
}

impl Server {
    pub fn new(cfg: &Config) -> Result<Self, Error> {
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
    type Item = Result<Connection, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.i.next() {
            Some(Ok(s)) => Some(Ok(Connection { s })),
            Some(Err(e)) => Some(Err(Error::IO(e))),
            None => None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Message {
    pub kind: Kind,
    pub data: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Kind {
    Command,
    History,
    Exit,
}
