use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashMap},
    env,
    fs::{self, File, exists},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use aws_lc_rs::{
    aead::{AES_256_GCM, Aad, Nonce, RandomizedNonceKey},
    cipher::AES_256_KEY_LEN,
    rand,
};
use base64::{Engine, prelude::BASE64_STANDARD};
use chrono::{DateTime, DurationRound, TimeDelta, Utc};
use itertools::Itertools;
use log::{debug, error};
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
pub(super) struct EncryptedChunk {
    #[serde(skip)]
    pub version: u8,
    pub start: DateTime<Utc>,
    #[serde(with = "serde_bytes")]
    nonce: Vec<u8>,
    #[serde(with = "serde_bytes")]
    data: Vec<u8>,
}

impl EncryptedChunk {
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

    pub(super) fn decrypt(mut self, key: &[u8]) -> Result<Chunk> {
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
