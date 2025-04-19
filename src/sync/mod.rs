use std::{fmt, fs::exists, path::PathBuf};

use log::debug;

use crate::{config::Config, error::Result};

mod dummy;
mod git;

pub trait Syncer: fmt::Debug + Send {
    fn refresh(&self) -> Result<PathBuf>;

    fn push_changes(&self, host: &str, force: bool) -> Result<()>;
}

pub fn get_syncer(cfg: &Config) -> Result<Box<dyn Syncer>> {
    let url = &cfg.sync.url;
    let path = cfg.sync_path();
    if !cfg.sync.enabled {
        debug!("Use dummy syncer, sync is disabled");
        Ok(Box::new(dummy::Dummy::new(&path)?))
    } else if exists(&path)? {
        debug!("Open existing git repo: {path:?}");
        Ok(Box::new(git::Git::existing(cfg)?))
    } else if url.is_empty() {
        debug!("Use dummy syncer, URL is not configured");
        Ok(Box::new(dummy::Dummy::new(&path)?))
    } else {
        debug!("Create new git repo at {path:?} from URL {url:?}");
        Ok(Box::new(git::Git::new(cfg)?))
    }
}
