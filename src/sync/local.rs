use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::error::Result;

use super::{LockedSyncer, Syncer};

#[derive(Debug, Clone)]
pub struct Local {
    path: PathBuf,
}

impl Local {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        fs::create_dir_all(path.as_ref())?;
        Ok(Self {
            path: path.as_ref().into(),
        })
    }
}

impl Syncer for Local {
    fn refresh(&self) -> Result<PathBuf> {
        Ok(self.path.clone())
    }

    fn push_changes(&self, _host: &str, _force: bool) -> Result<()> {
        Ok(())
    }

    fn lock<'a>(&'a self) -> Result<Box<dyn super::LockedSyncer + 'a>> {
        Ok(Box::new(self.clone()))
    }
}

impl<'a> LockedSyncer for Local {
    fn refresh(&self) -> Result<PathBuf> {
        Ok(self.path.clone())
    }

    fn push_changes(&self, _host: &str) -> Result<()> {
        Ok(())
    }

    fn unlock(&self) -> Result<()> {
        Ok(())
    }
}
