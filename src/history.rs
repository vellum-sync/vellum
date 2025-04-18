use std::{cmp::Ordering, collections::HashMap, fs, io::Write, path::Path};

use aws_lc_rs::aead::{AES_256_GCM, Aad, Nonce, RandomizedNonceKey};
use chrono::{DateTime, Utc};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{error::Result, sync::Syncer};

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: Uuid,
    pub ts: DateTime<Utc>,
    pub host: String,
    pub cmd: String,
}

impl Entry {
    fn new<H: Into<String>, C: Into<String>>(host: H, cmd: C) -> Self {
        Self {
            id: Uuid::now_v7(),
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

#[derive(Debug, Serialize, Deserialize)]
struct EncryptedChunk {
    start: DateTime<Utc>,
    nonce: Vec<u8>,
    data: Vec<u8>,
}

impl EncryptedChunk {
    fn encrypt(chunk: &Chunk, key: &[u8]) -> Result<Self> {
        let key = RandomizedNonceKey::new(&AES_256_GCM, key)?;
        let mut data = rmp_serde::to_vec(&chunk.entries)?;
        let nonce = key.seal_in_place_append_tag(Aad::empty(), &mut data)?;
        Ok(Self {
            start: chunk.start.clone(),
            nonce: nonce.as_ref().into(),
            data,
        })
    }

    fn decrypt(mut self, key: &[u8]) -> Result<Chunk> {
        let key = RandomizedNonceKey::new(&AES_256_GCM, key)?;
        let nonce = Nonce::try_assume_unique_for_key(&self.nonce)?;
        let data = key.open_in_place(nonce, Aad::empty(), &mut self.data)?;
        Ok(Chunk {
            start: self.start,
            entries: rmp_serde::from_slice(data)?,
        })
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
        for entry in fs::read_dir(&path)? {
            let entry = entry?;
            let file_name = entry.file_name();
            let host = file_name.to_string_lossy();
            if entry.path().is_dir() && host != self.host.as_str() {
                self.read_host(entry.path(), host)?;
            }
        }
        Ok(())
    }

    fn read_host<P: AsRef<Path>, S: Into<String>>(&mut self, path: P, host: S) -> Result<()> {
        panic!("not implemented")
    }

    fn write<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        // TODO(jp3): Get a real encryption key ...
        let key = vec![];

        // First we need to make sure that there is actually anything to write.
        let chunks = match self.history.get(&self.host) {
            Some(c) => c,
            None => return Ok(()),
        };

        for (day, chunks) in chunks
            .iter()
            .filter(|chunk| chunk.start < self.last_write)
            .chunk_by(|chunk| format!("{}", chunk.start.format("%Y-%m-%d")))
            .into_iter()
        {
            let filename = Path::new(path.as_ref()).join(day);
            let mut f = fs::File::options()
                .append(true)
                .create(true)
                .open(filename)?;
            for chunk in chunks.map(|chunk| EncryptedChunk::encrypt(chunk, &key)) {
                let data = rmp_serde::to_vec(&chunk?)?;
                let len = data.len() as u64;
                f.write_all(&len.to_be_bytes())?;
                f.write_all(&data)?;
            }
            f.flush()?;
        }

        Ok(())
    }
}
