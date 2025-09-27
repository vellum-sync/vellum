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

pub fn get_syncer(cfg: &Config) -> Result<(Box<dyn Syncer>, PathBuf)> {
    if cfg.sync.enabled {
        debug!("Using git Syncer");
        let s = git::Git::new(cfg)?;
        let path = s.path();
        Ok((Box::new(s), path))
    } else {
        debug!("Using local Syncer");
        let path = cfg.sync_path();
        let s = local::Local::new(&path)?;
        let path = s.path();
        Ok((Box::new(s), path))
    }
}
