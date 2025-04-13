use std::{
    fmt,
    fs::{File, exists},
    io::Write,
    path::{Path, PathBuf},
};

use git2::{Oid, Repository};
use log::debug;

use crate::{config::Config, error::Result};

#[derive(Debug)]
pub struct Version {
    oid: Oid,
}

#[derive(Debug)]
pub struct Data {
    pub version: Version,
    pub data: Vec<u8>,
}

pub trait Syncer: fmt::Debug + Send {
    fn store(&self, host: &str, data: &[u8]) -> Result<()>;

    fn get_newer(&self, host: &str, ver: Option<&Version>) -> Result<Option<Data>>;
}

pub struct Sync {
    path: PathBuf,
    cfg: Config,
    repo: Repository,
}

impl Sync {
    fn existing(cfg: &Config) -> Result<Self> {
        let path = cfg.sync_path();
        let repo = Repository::open(&path)?;
        Ok(Self {
            path,
            cfg: cfg.clone(),
            repo,
        })
    }

    fn new(cfg: &Config) -> Result<Self> {
        let path = cfg.sync_path();
        let repo = Repository::init(&path)?;
        Ok(Self {
            path,
            cfg: cfg.clone(),
            repo,
        })
    }
}

impl fmt::Debug for Sync {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sync{{cfg: {:?}, repo: {:?}", self.cfg, self.repo.path())
    }
}

impl Syncer for Sync {
    fn store(&self, host: &str, data: &[u8]) -> Result<()> {
        // TODO(jp3): implement this
        Ok(())
    }

    fn get_newer(&self, host: &str, ver: Option<&Version>) -> Result<Option<Data>> {
        // TODO(jp3): implement this
        Ok(None)
    }
}

#[derive(Debug)]
pub struct Dummy;

impl Dummy {
    fn new() -> Self {
        Self {}
    }
}

impl Syncer for Dummy {
    fn store(&self, _host: &str, _data: &[u8]) -> Result<()> {
        Ok(())
    }

    fn get_newer(&self, _host: &str, _ver: Option<&Version>) -> Result<Option<Data>> {
        Ok(None)
    }
}

pub fn get_syncer(cfg: &Config) -> Result<Box<dyn Syncer>> {
    let url = &cfg.sync.url;
    let path = cfg.sync_path();
    if !cfg.sync.enabled {
        debug!("Use dummy syncer, sync is disabled");
        Ok(Box::new(Dummy::new()))
    } else if exists(&path)? {
        debug!("Open existing git repo: {path:?}");
        Ok(Box::new(Sync::existing(cfg)?))
    } else if url.is_empty() {
        debug!("Use dummy syncer, URL is not configured");
        Ok(Box::new(Dummy::new()))
    } else {
        debug!("Create new git repo at {path:?} from URL {url:?}");
        Ok(Box::new(Sync::new(cfg)?))
    }
}
