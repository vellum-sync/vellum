use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::error::Result;

use super::{SyncGuard, Syncer};

#[derive(Debug, Clone)]
pub struct Dummy {
    path: PathBuf,
}

impl Dummy {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        fs::create_dir_all(path.as_ref())?;
        Ok(Self {
            path: path.as_ref().into(),
        })
    }
}

impl Syncer for Dummy {
    fn refresh(&self) -> Result<PathBuf> {
        Ok(self.path.clone())
    }

    fn push_changes(&self, _host: &str, _force: bool) -> Result<()> {
        Ok(())
    }

    fn lock<'a>(&'a self) -> Result<Box<dyn super::SyncGuard + 'a>> {
        Ok(Box::new(self.clone()))
    }
}

impl<'a> SyncGuard for Dummy {
    fn unlock(&self) -> Result<()> {
        Ok(())
    }
}
