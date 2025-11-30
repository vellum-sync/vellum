use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
    time::Duration,
};

use chrono::{DateTime, DurationRound, TimeDelta, Utc};
use itertools::Itertools;
use log::{debug, error};
use uuid::Uuid;

use crate::error::{Error, Result};

mod store;

use store::{Chunk, Store};
pub use store::{Entry, generate_key, get_key};

#[derive(Debug)]
pub struct History {
    host: String,
    store: Store,
    history: HashMap<String, Vec<Chunk>>,
    merged: Vec<Entry>,
    last_write: DateTime<Utc>,
}

impl History {
    fn new<H: Into<String>, S: AsRef<Path>>(host: H, state: S) -> Result<Self> {
        Ok(Self {
            host: host.into(),
            store: Store::new(state)?,
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
        self.write(path)?;
        self.write_active_chunk();
        Ok(())
    }

    pub fn sync<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        self.write(path.as_ref())?;
        self.write_active_chunk();
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
        self.write_active_chunk();
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
        self.write_active_chunk();
        Ok(())
    }

    pub fn load_entries(&mut self, entries: Vec<Entry>, all_hosts: bool) -> Result<usize> {
        if all_hosts {
            return Err(Error::from_str(
                "Loading from all hosts is currently not implemented",
            ));
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

        self.write_active_chunk();

        Ok(count)
    }

    pub fn rewrite_all_files<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        self.rebuild_chunks()?;
        self.store.rewrite_all_chunks(path, &self.history)?;
        self.last_write = Utc::now();
        self.write_active_chunk();
        Ok(())
    }

    fn active_chunk(&self) -> Option<&Chunk> {
        match self.history.get(&self.host) {
            Some(chunks) => match chunks.last() {
                Some(last) if last.start > self.last_write => Some(last),
                _ => None,
            },
            None => None,
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
        for entry in self.store.get_hosts(&path)? {
            let (host, path) = entry?;
            if empty || host != self.host {
                added |= self.read_host(path, host)?;
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

        let last_read = self.last_read(&host);
        let new_chunks = self.store.read_chunks(path, last_read)?;

        if new_chunks.is_empty() {
            debug!("added=false");
            return Ok(false);
        }

        let chunks = self.history.entry(host).or_default();
        chunks.extend(new_chunks);

        // we might have read chunks out of order, but we want them in time
        // order, so sort them.
        chunks.sort_unstable_by_key(|chunk| chunk.start);

        debug!("added=true");

        Ok(true)
    }

    fn read_active_chunk(&mut self) -> Result<()> {
        let chunks = self.history.entry(self.host.clone()).or_default();

        // this function should never be called when there is already an active chunk.
        if let Some(last) = chunks.last() {
            if last.start > self.last_write {
                return Err(Error::from_str(
                    "read_active_chunk called, but there is already an active chunk!",
                ));
            }
        }

        let chunks = self.store.read_state()?;

        if chunks.is_empty() {
            // there was nothing read, so we are done.
            return Ok(());
        }

        // We need to make sure that last_write is before the time in the first
        // chunk from the active file, since it hasn't be synced yet - so we
        // want the next sync to consider it new.
        self.last_write = chunks[0].start - Duration::from_secs(1);

        // Iterate the chunks and add up the number of entries before we merge
        // them into the host's chunk list.
        let added: usize = chunks.iter().map(|chunk| chunk.entries.len()).sum();

        // Sort the loaded chunks in the active chunk, creating it if needed.
        self.history
            .entry(self.host.clone())
            .or_default()
            .extend(chunks);

        if added > 0 {
            // if we have loaded any entries then we need to rebuild the
            // merged list.
            self.rebuild_merged();
        }

        Ok(())
    }

    fn write<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        // First we need to make sure that there is actually anything to write.
        let chunks = match self.history.get(&self.host) {
            Some(c) => c,
            None => return Ok(()),
        };

        self.store
            .write_chunks(path, &self.host, chunks, self.last_write)?;

        self.last_write = Utc::now();

        Ok(())
    }

    fn write_active_chunk(&self) {
        if let Err(e) = self.store.write_state(self.active_chunk()) {
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
