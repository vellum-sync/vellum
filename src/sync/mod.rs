use std::{fmt, path::PathBuf};

use log::debug;

use crate::{config::Config, error::Result};

mod git;
mod local;

pub trait Syncer: fmt::Debug + Send {
    fn refresh(&self) -> Result<PathBuf>;

    fn push_changes(&self, host: &str, force: bool) -> Result<()>;

    fn lock<'a>(&'a self) -> Result<Box<dyn LockedSyncer + 'a>>;
}

pub trait LockedSyncer: fmt::Debug {
    fn refresh(&self) -> Result<PathBuf>;

    fn push_changes(&self, host: &str) -> Result<()>;

    fn unlock(&self) -> Result<()>;
}

pub fn get_syncer(cfg: &Config) -> Result<Box<dyn Syncer>> {
    if cfg.sync.enabled {
        debug!("Using git Syncer");
        Ok(Box::new(git::Git::new(cfg)?))
    } else {
        debug!("Using local Syncer");
        let path = cfg.sync_path();
        Ok(Box::new(local::Local::new(&path)?))
    }
}
