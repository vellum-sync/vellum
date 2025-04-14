use std::{
    fmt,
    fs::{self, File, exists, read_dir},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use git2::{Commit, ErrorCode, Oid, Repository};
use log::debug;

use crate::{config::Config, error::Result};

use super::{Data, Syncer, Version};

pub struct Git {
    path: PathBuf,
    cfg: Config,
    repo: Repository,
}

impl Git {
    pub fn existing(cfg: &Config) -> Result<Self> {
        let path = cfg.sync_path();
        let repo = Repository::open(&path)?;
        Ok(Self {
            path,
            cfg: cfg.clone(),
            repo,
        })
    }

    pub fn new(cfg: &Config) -> Result<Self> {
        let path = cfg.sync_path();
        let repo = Repository::init(&path)?;
        Ok(Self {
            path,
            cfg: cfg.clone(),
            repo,
        })
    }

    fn pull(&self) -> Result<()> {
        // TODO(jp3): need to implement `pull -r`
        Ok(())
    }

    fn push(&self) -> Result<()> {
        // TODO(jp3): need to implement `push`
        Ok(())
    }

    fn tip(&self) -> Result<Option<Commit>> {
        let oid = match self.repo.head() {
            Ok(head) => head.target(),
            Err(e) => {
                if e.code() == ErrorCode::NotFound {
                    return Ok(None);
                }
                return Err(e.into());
            }
        };
        Ok(match oid {
            Some(id) => Some(self.repo.find_commit(id)?),
            None => None,
        })
    }

    fn commit(&self, message: &str, force: bool) -> Result<Option<Oid>> {
        let mut index = self.repo.index()?;
        let tree = self.repo.find_tree(index.write_tree()?)?;
        let author = self.repo.signature()?;
        let mut parents = Vec::with_capacity(1);
        let tip = self.tip()?;
        if let Some(tip) = tip.as_ref() {
            debug!("tree: {:?}, tip: {:?}", tree.id(), tip.tree_id());
            if tree.id() == tip.tree_id() && !force {
                // nothing has changed, so don't create a new commit unless
                // forced.
                return Ok(None);
            }
            parents.push(tip);
        }
        let commit = self
            .repo
            .commit(Some("HEAD"), &author, &author, message, &tree, &parents)?;
        debug!("Created commit {commit:?}");
        Ok(Some(commit))
    }
}

impl fmt::Debug for Git {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sync{{cfg: {:?}, repo: {:?}", self.cfg, self.repo.path())
    }
}

impl Syncer for Git {
    fn store(&self, host: &str, data: &[u8], force: bool) -> Result<()> {
        self.pull()?;

        let mut index = self.repo.index()?;
        let target = Path::new("hosts").join(host);
        let path = Path::new(&self.path).join(&target);
        fs::create_dir_all(path.parent().unwrap())?;
        {
            // write contents to file, and make sure it's on disk before calling
            // add_path.
            let mut f = File::create(&path)?;
            f.write_all(data)?;
            f.flush()?;
        }
        index.add_path(&target)?;
        index.write()?;

        if let Some(commit) = self.commit(&format!("update {host}"), force)? {
            self.push()?;
        }

        Ok(())
    }

    fn get_newer(&self, host: &str, ver: Option<&Version>) -> Result<Option<Data>> {
        self.pull()?;

        let target = Path::new("hosts").join(host);
        let path = Path::new(&self.path).join(&target);
        if !exists(&path)? {
            // there is no history for the specified host currently.
            return Ok(None);
        }
        // TODO(jp3): need to get the last modified version of the file
        let version = Version { oid: Oid::zero() };
        if let Some(prev) = ver {
            // TODO(jp3): we should check to see if the file has changed since
            // prev, and return None if not ...
        }
        let mut data = Vec::new();
        let mut f = File::open(&path)?;
        f.read_to_end(&mut data)?;
        Ok(Some(Data { version, data }))
    }

    fn get_external_hosts(&self, host: &str) -> Result<Vec<String>> {
        self.pull()?;

        let mut hosts = Vec::new();
        let path = Path::new(&self.path).join("hosts");

        for entry in read_dir(&path)? {
            let entry = entry?;
            if !entry.path().is_dir() && entry.file_name() != host {
                hosts.push(entry.file_name().to_string_lossy().to_string());
            }
        }

        Ok(hosts)
    }
}
