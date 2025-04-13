use std::{
    fmt,
    fs::{self, File, exists},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use git2::{Commit, ErrorCode, Oid, Repository};
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
}

impl fmt::Debug for Sync {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sync{{cfg: {:?}, repo: {:?}", self.cfg, self.repo.path())
    }
}

impl Syncer for Sync {
    fn store(&self, host: &str, data: &[u8]) -> Result<()> {
        // TODO(jp3): need to start with `pull -r`
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
        let tree = self.repo.find_tree(index.write_tree()?)?;
        let author = self.repo.signature()?;
        let mut parents = Vec::with_capacity(1);
        let tip = self.tip()?;
        if let Some(tip) = tip.as_ref() {
            debug!("tree: {:?}, tip: {:?}", tree.id(), tip.tree_id());
            if tree.id() == tip.tree_id() {
                // nothing has changed, so don't create a new commit.
                return Ok(());
            }
            parents.push(tip);
        }
        let commit = self.repo.commit(
            Some("HEAD"),
            &author,
            &author,
            &format!("update {host}"),
            &tree,
            &parents,
        )?;
        debug!("Created commit {commit:?}");
        // TODO(jp3): now need to push the commit.
        Ok(())
    }

    fn get_newer(&self, host: &str, ver: Option<&Version>) -> Result<Option<Data>> {
        // TODO(jp3): need to start with `pull -r`
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
