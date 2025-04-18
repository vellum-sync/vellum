use std::{
    cmp::Ordering,
    collections::HashMap,
    fs::{self, File},
    io::{self, Read, Write},
    path::Path,
};

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
            last_read: DateTime::from_timestamp_nanos(0),
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

    pub fn sync(&mut self, syncer: &dyn Syncer, force: bool) -> Result<()> {
        let update = syncer.start_update(&self.host)?;
        let path = update.path();
        self.write(&path)?;
        self.last_write = Utc::now();
        self.read(&path)?;
        self.last_read = Utc::now();
        update.finish(force)
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
        // TODO(jp3): Get a real encryption key ...
        let key: Vec<u8> = vec![];

        let last_read_day = format!("{}", self.last_read.format("%Y-%m-%d"));

        let chunks = self.history.entry(host.into()).or_default();

        for entry in fs::read_dir(&path)? {
            let entry = entry?;
            let day = entry.file_name();
            if day.to_string_lossy().as_ref() < last_read_day.as_str() {
                // skip any files that have already been read
                continue;
            }

            // read chunks from the file, ignoring any that we have already
            // read.
            let mut new_chunks = HistoryFile::open(entry.path())?
                .filter(|chunk| match chunk {
                    Ok(c) => c.start >= self.last_read,
                    Err(_) => true,
                })
                .map(|chunk| match chunk {
                    Ok(c) => c.decrypt(&key),
                    Err(e) => Err(e),
                })
                .collect::<Result<Vec<Chunk>>>()?;

            chunks.append(&mut new_chunks);
        }

        // we might have read chunks out of order, but we want them in time
        // order, so sort them.
        chunks.sort_unstable_by_key(|chunk| chunk.start);

        Ok(())
    }

    fn write<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        // TODO(jp3): Get a real encryption key ...
        let key = vec![];

        // First we need to make sure that there is actually anything to write.
        let chunks = match self.history.get(&self.host) {
            Some(c) => c,
            None => return Ok(()),
        };

        // make sure host directory exists
        let dir = Path::new(path.as_ref()).join(&self.host);
        fs::create_dir_all(&dir)?;

        for (day, chunks) in chunks
            .iter()
            .filter(|chunk| chunk.start < self.last_write)
            .chunk_by(|chunk| format!("{}", chunk.start.format("%Y-%m-%d")))
            .into_iter()
        {
            let mut f = File::options()
                .append(true)
                .create(true)
                .open(Path::new(&dir).join(day))?;
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

struct HistoryFile {
    f: File,
    complete: bool,
}

impl HistoryFile {
    fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        Ok(Self {
            f: File::open(path)?,
            complete: false,
        })
    }

    fn read(&mut self) -> Result<Option<EncryptedChunk>> {
        let mut buf = [0 as u8; 8];
        let mut read = 0;

        while read < buf.len() {
            let n = match self.f.read(&mut buf[read..]) {
                Ok(n) => n,
                Err(e) => match e.kind() {
                    io::ErrorKind::Interrupted => continue,
                    _ => return Err(e.into()),
                },
            };
            if n == 0 {
                return Ok(None);
            };
            read += n
        }

        let len = u64::from_be_bytes(buf);

        let mut data = vec![0u8; len as usize];
        self.f.read_exact(&mut data)?;

        Ok(Some(rmp_serde::from_slice(&data)?))
    }
}

impl Iterator for HistoryFile {
    type Item = Result<EncryptedChunk>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.complete {
            return None;
        }
        match self.read() {
            Ok(Some(c)) => Some(Ok(c)),
            Ok(None) => {
                self.complete = true;
                None
            }
            Err(e) => {
                self.complete = true;
                Some(Err(e))
            }
        }
    }
}
