use std::{
    cmp::Ordering,
    collections::HashMap,
    env,
    fs::{self, File, ReadDir, exists},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use aws_lc_rs::{
    aead::{AES_256_GCM, Aad, Nonce, RandomizedNonceKey},
    cipher::AES_256_KEY_LEN,
    rand,
};
use base64::{Engine, prelude::BASE64_STANDARD};
use chrono::{DateTime, Utc};
use itertools::Itertools;
use log::{debug, warn};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: Uuid,
    pub ts: DateTime<Utc>,
    pub host: String,
    pub cmd: String,
    pub path: String,
    pub session: String,
}

impl Entry {
    pub(super) fn new<H: Into<String>, C: Into<String>, P: Into<String>, S: Into<String>>(
        host: H,
        cmd: C,
        path: P,
        session: S,
    ) -> Self {
        Self::existing(Uuid::now_v7(), host, cmd, path, session)
    }

    pub(super) fn existing<
        I: Into<Uuid>,
        H: Into<String>,
        C: Into<String>,
        P: Into<String>,
        S: Into<String>,
    >(
        id: I,
        host: H,
        cmd: C,
        path: P,
        session: S,
    ) -> Self {
        Self {
            id: id.into(),
            ts: Utc::now(),
            host: host.into(),
            cmd: cmd.into(),
            path: path.into(),
            session: session.into(),
        }
    }
}

impl Ord for Entry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.ts
            .cmp(&other.ts)
            .then(self.host.cmp(&other.host))
            .then(self.cmd.cmp(&other.cmd))
            .then(self.path.cmp(&other.path))
    }
}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug)]
pub(super) struct Chunk {
    pub start: DateTime<Utc>,
    pub entries: Vec<Entry>,
}

impl Chunk {
    pub(super) fn new() -> Self {
        Self {
            start: Utc::now(),
            entries: Vec::new(),
        }
    }

    pub(super) fn with_start(start: DateTime<Utc>) -> Self {
        Self {
            start,
            entries: Vec::new(),
        }
    }

    pub(super) fn push(&mut self, entry: Entry) {
        self.entries.push(entry);
    }

    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }

    fn read(start: DateTime<Utc>, data: &[u8]) -> Result<Self> {
        Ok(Self {
            start,
            entries: rmp_serde::from_slice(data)?,
        })
    }
}

const CURRENT_CHUNK_VERSION: u8 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct EncryptedChunk {
    #[serde(skip)]
    version: u8,
    start: DateTime<Utc>,
    #[serde(with = "serde_bytes")]
    nonce: Vec<u8>,
    #[serde(with = "serde_bytes")]
    data: Vec<u8>,
}

impl EncryptedChunk {
    fn read(version: u8, data: &[u8]) -> Result<Option<Self>> {
        match version {
            0 | CURRENT_CHUNK_VERSION => {
                let mut chunk: EncryptedChunk = rmp_serde::from_slice(data)?;
                chunk.version = version;
                Ok(Some(chunk))
            }
            v => {
                warn!("Ignoring chunk of unknown version {v}");
                Ok(None)
            }
        }
    }

    fn encrypt(chunk: &Chunk, key: &[u8]) -> Result<Self> {
        let key = RandomizedNonceKey::new(&AES_256_GCM, key)?;
        let mut data = rmp_serde::to_vec(&chunk.entries)?;
        let nonce = key.seal_in_place_append_tag(Aad::empty(), &mut data)?;
        Ok(Self {
            version: CURRENT_CHUNK_VERSION,
            start: chunk.start,
            nonce: nonce.as_ref().into(),
            data,
        })
    }

    fn decrypt(mut self, key: &[u8]) -> Result<Chunk> {
        let key = RandomizedNonceKey::new(&AES_256_GCM, key)?;
        let nonce = Nonce::try_assume_unique_for_key(&self.nonce)?;
        let data = key.open_in_place(nonce, Aad::empty(), &mut self.data)?;
        match self.version {
            0 => v0::read(self.start, data),
            CURRENT_CHUNK_VERSION => Chunk::read(self.start, data),
            v => Err(Error::Generic(format!("Invalid Chunk version: {v}"))),
        }
    }
}

pub fn generate_key() -> Result<String> {
    let mut buf = [0_u8; AES_256_KEY_LEN];
    rand::fill(&mut buf)?;
    Ok(BASE64_STANDARD.encode(buf))
}

pub fn get_key() -> Result<Vec<u8>> {
    let vellum_key = env::var("VELLUM_KEY")?;
    let key = BASE64_STANDARD.decode(&vellum_key)?;
    if key.len() != AES_256_KEY_LEN {
        return Err(Error::Generic(format!(
            "key should be {AES_256_KEY_LEN} bytes, got {}",
            key.len()
        )));
    }
    Ok(key)
}

pub(super) fn write_chunk(f: &mut File, chunk: &Chunk, key: &[u8]) -> Result<()> {
    let chunk = EncryptedChunk::encrypt(chunk, key)?;
    let data = rmp_serde::to_vec(&chunk)?;
    let len = data.len() as u64;
    let header = len | ((chunk.version as u64) << 56);
    f.write_all(&header.to_be_bytes())?;
    f.write_all(&data)?;
    Ok(())
}

#[derive(Debug)]
pub(super) struct Store {
    key: Vec<u8>,
    state: PathBuf,
}

impl Store {
    pub(super) fn new<S: AsRef<Path>>(state: S) -> Result<Self> {
        let key = get_key()?;
        let state_dir = state.as_ref();
        fs::create_dir_all(state_dir)?;
        let state = Path::new(state_dir).join("history.chunk");
        Ok(Self { key, state })
    }

    pub(super) fn read_state(&self) -> Result<Vec<Chunk>> {
        if !exists(&self.state)? {
            debug!(
                "active chunk file {:?} not found, skipping active chunks load",
                self.state
            );
            return Ok(Vec::new());
        }

        let path = self.state.clone();

        debug!("load active chunks from {path:?}");

        let mut f = HistoryFile::open(path)?;

        let chunk = match f.read()? {
            Some(e) => e.decrypt(&self.key)?,
            None => return Ok(Vec::new()),
        };

        debug!(
            "found active chunk from {} with {} entries",
            chunk.start,
            chunk.entries.len()
        );

        let mut chunks = vec![chunk];

        // there should only ever be one chunk in the active chunk file, but if
        // there are any extra chunks, load them too.
        while let Some(e) = f.read()? {
            let chunk = e.decrypt(&self.key)?;
            debug!(
                "found active chunk from {} with {} entries",
                chunk.start,
                chunk.entries.len()
            );
            chunks.push(chunk);
        }

        Ok(chunks)
    }

    pub(super) fn write_state(&self, chunk: Option<&Chunk>) -> Result<()> {
        let path = self.state.clone();
        let mut f = File::create(path)?;

        if let Some(chunk) = chunk {
            write_chunk(&mut f, chunk, &self.key)?;
        }

        f.flush()?;
        Ok(())
    }

    pub(super) fn get_hosts<P: AsRef<Path>>(&self, path: P) -> Result<HostIterator> {
        HostIterator::new(path)
    }

    pub(super) fn read_chunks<P: AsRef<Path>>(
        &self,
        path: P,
        last_read: DateTime<Utc>,
    ) -> Result<Vec<Chunk>> {
        let mut chunks = Vec::new();
        let last_read_day = format!("{}", last_read.format("%Y-%m-%d"));

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
                    Ok(c) => c.start > last_read,
                    Err(_) => true,
                })
                .map(|chunk| match chunk {
                    Ok(c) => c.decrypt(&self.key),
                    Err(e) => Err(e),
                })
                .collect::<Result<Vec<Chunk>>>()?;

            if !new_chunks.is_empty() {
                // we only need to do anything if we read some new chunks
                chunks.append(&mut new_chunks);
            }
        }

        Ok(chunks)
    }

    pub(super) fn write_chunks<P: AsRef<Path>>(
        &self,
        path: P,
        host: &str,
        chunks: &Vec<Chunk>,
        last_write: DateTime<Utc>,
    ) -> Result<()> {
        debug!("We have {} total chunks", chunks.len());

        let mut entries = 0;

        // make sure host directory exists
        let dir = Path::new(path.as_ref()).join(host);
        fs::create_dir_all(&dir)?;

        for (day, chunks) in chunks
            .iter()
            .filter(|chunk| chunk.start > last_write && !chunk.entries.is_empty())
            .chunk_by(|chunk| format!("{}", chunk.start.format("%Y-%m-%d")))
            .into_iter()
        {
            debug!("write chunks for {day}");
            let mut f = File::options()
                .append(true)
                .create(true)
                .open(Path::new(&dir).join(day))?;
            for chunk in chunks {
                entries += chunk.entries.len();
                write_chunk(&mut f, chunk, &self.key)?;
            }
            f.flush()?;
        }

        debug!("Wrote total of {entries} new entries");

        Ok(())
    }

    pub(super) fn rewrite_all_chunks<P: AsRef<Path>>(
        &self,
        path: P,
        history: &HashMap<String, Vec<Chunk>>,
    ) -> Result<()> {
        fs::remove_dir_all(path.as_ref())?;
        // since we have removed the files, use the epoch as the last_write time
        for (host, chunks) in history.iter() {
            self.write_chunks(path.as_ref(), host, chunks, DateTime::UNIX_EPOCH)?;
        }
        Ok(())
    }
}

pub(super) struct HostIterator {
    rd: ReadDir,
}

impl HostIterator {
    fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        Ok(Self {
            rd: fs::read_dir(path)?,
        })
    }
}

impl Iterator for HostIterator {
    type Item = Result<(String, PathBuf)>;

    fn next(&mut self) -> Option<Self::Item> {
        let entry = match self.rd.next() {
            Some(Ok(entry)) => entry,
            Some(Err(e)) => return Some(Err(e.into())),
            None => return None,
        };
        let path = entry.path();
        if !path.is_dir() {
            // skip non-directory entries
            return self.next();
        }
        let file_name = entry.file_name();
        let host = file_name.to_string_lossy();
        return Some(Ok((host.into(), path)));
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
        let mut buf = [0_u8; 8];
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

        // chunk header is 8 bytes, 1st byte is chunk version, and remaining 7
        // bytes are the data length.
        let header = u64::from_be_bytes(buf);
        let len = header & 0x00ffffffffffffff;
        let version = ((header & 0xff00000000000000) >> 56) as u8;

        let mut data = vec![0u8; len as usize];
        self.f.read_exact(&mut data)?;

        match EncryptedChunk::read(version, &data)? {
            Some(chunk) => Ok(Some(chunk)),
            None => self.read(),
        }
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

mod v0 {
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Serialize};
    use uuid::Uuid;

    use crate::error::Result;

    use super::Chunk;

    #[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
    pub struct Entry {
        pub id: Uuid,
        pub ts: DateTime<Utc>,
        pub host: String,
        pub cmd: String,
        pub session: String,
    }

    impl Entry {
        fn convert(self) -> Result<super::Entry> {
            Ok(super::Entry {
                id: self.id,
                ts: self.ts,
                host: self.host,
                cmd: self.cmd,
                path: "".to_string(),
                session: self.session,
            })
        }
    }

    pub(super) fn read(start: DateTime<Utc>, data: &[u8]) -> Result<Chunk> {
        let entries: Vec<Entry> = rmp_serde::from_slice(data)?;
        Ok(super::Chunk {
            start,
            entries: entries
                .into_iter()
                .map(|e| e.convert())
                .collect::<Result<_>>()?,
        })
    }
}
