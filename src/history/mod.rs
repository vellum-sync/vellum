use std::{
    collections::{BTreeMap, HashMap},
    fs::{self, File, exists},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use chrono::{DateTime, DurationRound, TimeDelta, Utc};
use itertools::Itertools;
use log::{debug, error};
use uuid::Uuid;

use crate::error::{Error, Result};

mod store;

use store::{Chunk, EncryptedChunk, write_chunk};
pub use store::{Entry, generate_key, get_key};

#[derive(Debug)]
pub struct History {
    host: String,
    state: PathBuf,
    history: HashMap<String, Vec<Chunk>>,
    merged: Vec<Entry>,
    last_write: DateTime<Utc>,
}

impl History {
    fn new<H: Into<String>, S: AsRef<Path>>(host: H, state: S) -> Result<Self> {
        let state_dir = state.as_ref();
        fs::create_dir_all(state_dir)?;
        let state = Path::new(state.as_ref()).join("history.chunk");
        Ok(Self {
            host: host.into(),
            state,
            history: HashMap::new(),
            merged: Vec::new(),
            last_write: Utc::now(),
        })
    }

    pub fn load<H: Into<String>, S: AsRef<Path>, P: AsRef<Path>>(
        host: H,
        state: S,
        path: P,
    ) -> Result<Self> {
        let mut s = Self::new(host, state)?;
        s.read(path)?;
        s.read_active_chunk()?;
        Ok(s)
    }

    pub fn save<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        self.write(path, &self.host)?;
        self.last_write = Utc::now();
        self.write_active();
        Ok(())
    }

    pub fn sync<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        self.write(path.as_ref(), &self.host)?;
        self.last_write = Utc::now();
        self.write_active();
        self.read(path.as_ref())
    }

    pub fn history(&self) -> Vec<Entry> {
        self.merged.clone()
    }

    pub fn add<C: Into<String>, P: Into<String>, S: Into<String>>(
        &mut self,
        cmd: C,
        path: P,
        session: S,
    ) {
        let entry = Entry::new(&self.host, cmd, path, session);
        self.get_active_chunk().push(entry.clone());
        self.merged.push(entry);
        self.write_active();
    }

    pub fn update<I: Into<Uuid>, C: Into<String>, S: Into<String>>(
        &mut self,
        id: I,
        cmd: C,
        session: S,
    ) -> Result<()> {
        let id = id.into();
        if !self.merged.iter().any(|entry| entry.id == id) {
            return Err(Error::Generic(format!("unknown ID: {id}")));
        }
        let entry = Entry::existing(id, &self.host, cmd, "", session);
        self.get_active_chunk().push(entry);
        self.rebuild_merged();
        self.write_active();
        Ok(())
    }

    pub fn load_entries(&mut self, entries: Vec<Entry>, all_hosts: bool) -> Result<usize> {
        if all_hosts {
            return Err(Error::Generic(format!(
                "Loading from all hosts is currently not implemented"
            )));
        }

        let mut current: BTreeMap<Uuid, String> = BTreeMap::new();

        for entry in self.merged.iter() {
            current.insert(entry.id, entry.cmd.clone());
        }

        let host = self.host.clone();

        let active = self.get_active_chunk();
        let before = active.len();

        for entry in entries {
            debug!("loaded entry: {entry:?}");
            if entry.host != host {
                // we only support loading entries for the current host, since
                // the server only "owns" entries for that host (adding entries
                // for other hosts would _require_ running a rebuild - since
                // otherwise we can't update files for other hosts).
                continue;
            }
            match current.get(&entry.id) {
                None => active.push(entry),
                Some(cmd) if cmd != &entry.cmd => active.push(entry),
                _ => (),
            }
        }

        let count = active.len() - before;
        debug!("added {count} new/updated entries");

        if count == 0 {
            // If there are no new entries, then there is nothing to do
            return Ok(0);
        }

        // whilst we are throwing all the "new" entries in the active chunk,
        // they may be updates, and may be out of order. So we want to rebuild
        // merged rather than assume that we can append the entries.
        self.rebuild_merged();

        self.write_active();

        Ok(count)
    }

    pub fn rewrite_all_files<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        self.rebuild_chunks()?;
        fs::remove_dir_all(path.as_ref())?;
        // since we have removed the files, reset the last_write time
        self.last_write = DateTime::UNIX_EPOCH;
        for (host, _) in self.history.iter() {
            self.write(path.as_ref(), host)?;
        }
        self.last_write = Utc::now();
        self.write_active();
        Ok(())
    }

    fn active_chunk(&mut self) -> Option<&mut Chunk> {
        let chunks = self.history.entry(self.host.clone()).or_default();
        // create a new chunk if chunks is empty, or if the most recent chunk
        // has already been written.
        match chunks.last_mut() {
            Some(last) if last.start > self.last_write => Some(last),
            _ => None,
        }
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

    fn read<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let mut added = false;

        // if we have no history data at all, then we want to read our own past
        // history too, as this is probably a new server start.
        let empty = self.history.is_empty();

        // read any new data from disk
        for entry in fs::read_dir(&path)? {
            let entry = entry?;
            let file_name = entry.file_name();
            let host = file_name.to_string_lossy();
            if entry.path().is_dir() && (empty || host != self.host.as_str()) {
                added |= self.read_host(entry.path(), host)?;
            }
        }

        // if we added any new entries, then we need to rebuild merged
        if added {
            self.rebuild_merged();
        }

        Ok(())
    }

    fn last_read(&self, host: &str) -> DateTime<Utc> {
        let epoch = DateTime::from_timestamp_nanos(0);
        let chunks = match self.history.get(host) {
            Some(c) => c,
            None => return epoch,
        };
        match chunks.iter().last() {
            Some(c) => c.start,
            None => epoch,
        }
    }

    fn read_host<P: AsRef<Path>, S: Into<String>>(&mut self, path: P, host: S) -> Result<bool> {
        let host = host.into();
        debug!("read chunks for {host}");

        let key = get_key()?;

        let last_read = self.last_read(&host);
        let last_read_day = format!("{}", last_read.format("%Y-%m-%d"));

        let chunks = self.history.entry(host).or_default();

        let mut added = false;
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
                    Ok(c) => c.decrypt(&key),
                    Err(e) => Err(e),
                })
                .collect::<Result<Vec<Chunk>>>()?;

            if !new_chunks.is_empty() {
                // we only need to do anything if we read some new chunks
                chunks.append(&mut new_chunks);
                added = true;
            }
        }

        // we might have read chunks out of order, but we want them in time
        // order, so sort them.
        chunks.sort_unstable_by_key(|chunk| chunk.start);

        debug!("added={added}");

        Ok(added)
    }

    fn read_active_chunk(&mut self) -> Result<()> {
        let chunks = self.history.entry(self.host.clone()).or_default();

        // this function should never be called when there is already an active chunk.
        if let Some(last) = chunks.last() {
            if last.start > self.last_write {
                return Err(Error::Generic(format!(
                    "read_active_chunk called, but there is already an active chunk!"
                )));
            }
        }

        let path = self.state.clone();

        if !exists(&path)? {
            debug!("active chunk file {path:?} not found, skipping active chunks load");
            return Ok(());
        }

        debug!("load active chunks from {path:?}");

        let key = get_key()?;
        let mut f = HistoryFile::open(path)?;

        let chunk = match f.read()? {
            Some(e) => e.decrypt(&key)?,
            None => return Ok(()),
        };

        debug!(
            "found active chunk from {} with {} entries",
            chunk.start,
            chunk.entries.len()
        );

        let mut added = chunk.entries.len();

        // we need to make sure that last_write is before the time in the first
        // chunk from the active file, since it hasn't be synced yet - so we
        // want the next sync to consider it new.
        self.last_write = chunk.start - Duration::from_secs(1);

        chunks.push(chunk);

        // there should only ever be one chunk in the active chunk file, but if
        // there are any extra chunks, load them too.
        while let Some(e) = f.read()? {
            let chunk = e.decrypt(&key)?;
            debug!(
                "found active chunk from {} with {} entries",
                chunk.start,
                chunk.entries.len()
            );
            added += chunk.entries.len();
            chunks.push(chunk);
        }

        if added > 0 {
            // if we have loaded any entries then we need to rebuild the
            // merged list.
            self.rebuild_merged();
        }

        Ok(())
    }

    fn write<P: AsRef<Path>>(&self, path: P, host: &str) -> Result<()> {
        let key = get_key()?;

        // First we need to make sure that there is actually anything to write.
        let chunks = match self.history.get(host) {
            Some(c) => c,
            None => return Ok(()),
        };

        debug!("We have {} total chunks", chunks.len());

        let mut entries = 0;

        // make sure host directory exists
        let dir = Path::new(path.as_ref()).join(host);
        fs::create_dir_all(&dir)?;

        for (day, chunks) in chunks
            .iter()
            .filter(|chunk| chunk.start > self.last_write && !chunk.entries.is_empty())
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
                write_chunk(&mut f, chunk, &key)?;
            }
            f.flush()?;
        }

        debug!("Wrote total of {entries} new entries");

        Ok(())
    }

    fn try_write_active(&mut self) -> Result<()> {
        let key = get_key()?;
        let path = self.state.clone();
        let mut f = File::create(path)?;

        if let Some(chunk) = self.active_chunk() {
            write_chunk(&mut f, chunk, &key)?;
        }

        f.flush()?;
        Ok(())
    }

    fn write_active(&mut self) -> () {
        if let Err(e) = self.try_write_active() {
            error!("Failed to write active chunk: {e}");
        }
    }

    fn rebuild_merged(&mut self) {
        let mut entries: BTreeMap<Uuid, Vec<Entry>> = BTreeMap::new();

        for (_, chunks) in self.history.iter() {
            for chunk in chunks {
                chunk
                    .entries
                    .iter()
                    .for_each(|entry| entries.entry(entry.id).or_default().push(entry.clone()));
            }
        }

        let mut new_merged: Vec<Entry> = entries
            .into_values()
            .map(collapse_entries)
            .filter(|entry| !entry.cmd.is_empty())
            .collect();

        new_merged.sort();
        self.merged = new_merged;
    }

    fn get_chunk_by_hour<'a>(
        &self,
        history: &'a mut HashMap<String, Vec<Chunk>>,
        host: String,
        ts: &DateTime<Utc>,
    ) -> Result<&'a mut Chunk> {
        let hour = ts.duration_trunc(TimeDelta::hours(1))?;
        let chunks = history.entry(host).or_default();
        // create a new chunk if chunks is empty, or if the most recent chunk
        // has already been written.
        match chunks.last() {
            Some(last) if last.start == hour => (),
            _ => chunks.push(Chunk::with_start(hour)),
        };
        // at this point chunks will *always* have at least one entry, so
        // last_mut will never return None.
        Ok(chunks.last_mut().unwrap())
    }

    fn rebuild_chunks(&mut self) -> Result<()> {
        let mut new_history = HashMap::new();

        for entry in self.merged.iter() {
            let chunk = self.get_chunk_by_hour(&mut new_history, entry.host.clone(), &entry.ts)?;
            chunk.push(entry.clone());
        }

        self.history = new_history;

        Ok(())
    }
}

fn collapse_entries(entries: Vec<Entry>) -> Entry {
    if entries.len() == 1 {
        return entries.into_iter().next().unwrap();
    }
    let mut entries = entries.into_iter().sorted();
    // we know that we must have at least two entries, so we just unwrap the
    // Options.
    let mut first = entries.next().unwrap();
    let last = entries.next_back().unwrap();
    first.cmd = last.cmd;
    first
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

        let header = u64::from_be_bytes(buf);
        let len = header & 0x00ffffffffffffff;

        let mut data = vec![0u8; len as usize];
        self.f.read_exact(&mut data)?;

        let mut chunk: EncryptedChunk = rmp_serde::from_slice(&data)?;
        chunk.version = ((header & 0xff00000000000000) >> 56) as u8;

        Ok(Some(chunk))
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
