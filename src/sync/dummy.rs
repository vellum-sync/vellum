use crate::error::Result;

use super::{Data, Syncer, Version};

#[derive(Debug)]
pub struct Dummy;

impl Dummy {
    pub fn new() -> Self {
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

    fn get_external_hosts(&self, _host: &str) -> Result<Vec<String>> {
        Ok(vec![])
    }
}
