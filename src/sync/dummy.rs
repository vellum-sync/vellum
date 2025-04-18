use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::error::Result;

use super::{Data, Syncer, Update, Version};

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
    fn store(&self, _host: &str, _data: &[u8], _force: bool) -> Result<()> {
        Ok(())
    }

    fn get_newer(&self, _host: &str, _ver: Option<&Version>) -> Result<Option<Data>> {
        Ok(None)
    }

    fn get_external_hosts(&self, _host: &str) -> Result<Vec<String>> {
        Ok(vec![])
    }

    fn start_update(&self, _host: &str) -> Result<Box<dyn Update>> {
        Ok(Box::new(self.clone()))
    }

    fn refresh(&self) -> Result<PathBuf> {
        Ok(self.path.clone())
    }
}

impl Update for Dummy {
    fn path(&self) -> PathBuf {
        self.path.clone()
    }

    fn finish(self: Box<Self>, _force: bool) -> Result<()> {
        Ok(())
    }
}
