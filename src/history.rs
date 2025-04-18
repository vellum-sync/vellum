use std::{cmp::Ordering, collections::HashMap, path::Path};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{error::Result, sync::Syncer};

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub ts: DateTime<Utc>,
    pub host: String,
    pub cmd: String,
}

impl Entry {
    fn new<H: Into<String>, C: Into<String>>(host: H, cmd: C) -> Self {
        Self {
            ts: Utc::now(),
            host: host.into(),
            cmd: cmd.into(),
        }
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
struct Chunk {
    start: DateTime<Utc>,
    entries: Vec<Entry>,
}

impl Chunk {
    fn new() -> Self {
        Self {
            start: Utc::now(),
            entries: Vec::new(),
        }
    }

    fn push(&mut self, entry: Entry) {
        self.entries.push(entry);
    }
}

pub struct History {
    host: String,
    history: HashMap<String, Vec<Chunk>>,
    merged: Vec<Entry>,
    last_write: DateTime<Utc>,
    last_read: DateTime<Utc>,
}

impl History {
    fn new<S: Into<String>>(host: S) -> Self {
        Self {
            host: host.into(),
            history: HashMap::new(),
            merged: Vec::new(),
            last_write: Utc::now(),
            last_read: Utc::now(),
        }
    }

    pub fn load<S: Into<String>>(host: S, syncer: &dyn Syncer) -> Result<Self> {
        let mut s = Self::new(host);
        s.update(syncer)?;
        Ok(s)
    }

    pub fn save(&mut self, syncer: &dyn Syncer, force: bool) -> Result<()> {
        let update = syncer.start_update(&self.host)?;
        self.write(update.path())?;
        self.last_write = Utc::now();
        update.finish(force)
    }

    pub fn update(&mut self, syncer: &dyn Syncer) -> Result<()> {
        let path = syncer.refresh()?;
        self.read(path)?;
        self.last_read = Utc::now();
        Ok(())
    }

    pub fn history(&self) -> Vec<Entry> {
        self.merged.clone()
    }

    fn get_active_chunk(&mut self) -> &mut Chunk {
        let chunks = self.history.entry(self.host.clone()).or_default();
        // create a new chunk if chunks is empty, or if the most recent chunk
        // has already been written.
        match chunks.last() {
            Some(last) if last.start > self.last_write => (),
            _ => chunks.push(Chunk::new()),
        };
        // at this point chunks will *always* have at least one entry, so
        // last_mut will never return None.
        chunks.last_mut().unwrap()
    }

    pub fn add<S: Into<String>>(&mut self, cmd: S) {
        let entry = Entry::new(&self.host, cmd);
        self.get_active_chunk().push(entry.clone());
        self.merged.push(entry);
    }

    fn read<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        panic!("not implemented")
    }

    fn write<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        panic!("not implements")
    }
}
