use std::{fmt, fs::exists};

use git2::Oid;
use log::debug;

use crate::{config::Config, error::Result};

mod dummy;
mod git;

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

    fn get_external_hosts(&self, host: &str) -> Result<Vec<String>>;
}

pub fn get_syncer(cfg: &Config) -> Result<Box<dyn Syncer>> {
    let url = &cfg.sync.url;
    let path = cfg.sync_path();
    if !cfg.sync.enabled {
        debug!("Use dummy syncer, sync is disabled");
        Ok(Box::new(dummy::Dummy::new()))
    } else if exists(&path)? {
        debug!("Open existing git repo: {path:?}");
        Ok(Box::new(git::Git::existing(cfg)?))
    } else if url.is_empty() {
        debug!("Use dummy syncer, URL is not configured");
        Ok(Box::new(dummy::Dummy::new()))
    } else {
        debug!("Create new git repo at {path:?} from URL {url:?}");
        Ok(Box::new(git::Git::new(cfg)?))
    }
}
