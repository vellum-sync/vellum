use std::{
    fmt,
    fs::{self, File, exists, read_dir},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use git2::{
    Commit, Cred, ErrorCode, FetchOptions, Oid, PushOptions, Rebase, RebaseOptions,
    RemoteCallbacks, Repository,
};
use log::debug;

use crate::{
    config::Config,
    error::{Error, Result},
};

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

    fn fetch(&self) -> Result<bool> {
        debug!("start fetch ...");

        let mut changes = false;

        let head_ref = self.repo.head()?.resolve()?;
        let ref_name = head_ref
            .name()
            .ok_or_else(|| Error::Generic(format!("unable to resolve HEAD")))?;

        let git_config = git2::Config::open_default()?;

        let mut cbs = RemoteCallbacks::new();
        cbs.credentials(|url, username, types| {
            debug!(
                "trying to find credentials for {url} (username: {username:?}, types: {types:?})"
            );
            if types.is_default() {
                Cred::default()
            } else if types.is_ssh_key() {
                let username = username
                    .ok_or_else(|| git2::Error::from_str("missing username for ssh auth"))?;
                if !self.cfg.sync.ssh_key.is_empty() {
                    let privatekey = Path::new(&self.cfg.sync.ssh_key);
                    Cred::ssh_key(username, None, privatekey, None)
                } else {
                    Cred::ssh_key_from_agent(username)
                }
            } else if types.is_user_pass_plaintext() {
                Cred::credential_helper(&git_config, url, username)
            } else {
                Err(git2::Error::from_str(&format!(
                    "no supported auth methods available: {types:?}"
                )))
            }
        })
        .update_tips(|name, old, new| {
            debug!("update tip: name: {name} old: {old:?} new: {new:?}");
            if name == ref_name {
                changes = true;
            }
            true
        });

        let mut opts = FetchOptions::new();
        opts.remote_callbacks(cbs);

        let mut remote = self.repo.find_remote("origin")?;

        remote.fetch::<&str>(&[ref_name], Some(&mut opts), None)?;

        // make sure that the update_tips callback is gone, since it implicitly
        // borrows changes.
        drop(opts);

        Ok(changes)
    }

    fn rebase(&self) -> Result<()> {
        let head = self.repo.head()?;
        let branch = self.repo.reference_to_annotated_commit(&head)?;

        let upstream_ref = self.repo.find_reference(
            self.repo
                .branch_upstream_name(branch.refname().unwrap())?
                .as_str()
                .unwrap(),
        )?;
        let upstream = self.repo.reference_to_annotated_commit(&upstream_ref)?;

        debug!(
            "start rebase of {:?} upstream {:?}",
            branch.refname(),
            upstream.refname()
        );

        let mut opts = RebaseOptions::new();
        opts.inmemory(false);

        let mut rebase = self
            .repo
            .rebase(Some(&branch), Some(&upstream), None, Some(&mut opts))?;

        if let Err(e) = self.run_rebase(&mut rebase) {
            // TODO(jp3): what do we do if abort fails? we are already handling
            // an error ...
            let _ = rebase.abort();
            return Err(e);
        }

        Ok(rebase.finish(None)?)
    }

    fn run_rebase(&self, rebase: &mut Rebase) -> Result<()> {
        let committer = self.repo.signature()?;

        loop {
            let operation = match rebase.next() {
                Some(Ok(op)) => op,
                Some(Err(e)) => return Err(Error::Git(e)),
                None => return Ok(()),
            };
            debug!("rebase op {:?}: {}", operation.kind(), operation.id());
            // let oid = rebase.commit(None, &committer, None)?;
            // debug!("updated {} -> {}", operation.id(), oid);
        }
    }

    fn pull(&self) -> Result<()> {
        if self.fetch()? {
            self.rebase()?;
        }
        Ok(())
    }

    fn push(&self) -> Result<()> {
        match self.try_push() {
            Err(Error::Git(e)) => {
                if e.code() == ErrorCode::NotFastForward {
                    debug!("push failed due to NotFasForward, try rebase ...");
                    self.rebase()?;
                    self.try_push()
                } else {
                    Err(Error::Git(e))
                }
            }
            r => r,
        }
    }

    fn try_push(&self) -> Result<()> {
        debug!("start push ...");

        let git_config = git2::Config::open_default()?;

        let mut cbs = RemoteCallbacks::new();
        cbs.credentials(|url, username, types| {
            debug!(
                "trying to find credentials for {url} (username: {username:?}, types: {types:?})"
            );
            if types.is_default() {
                Cred::default()
            } else if types.is_ssh_key() {
                let username = username
                    .ok_or_else(|| git2::Error::from_str("missing username for ssh auth"))?;
                if !self.cfg.sync.ssh_key.is_empty() {
                    let privatekey = Path::new(&self.cfg.sync.ssh_key);
                    Cred::ssh_key(username, None, privatekey, None)
                } else {
                    Cred::ssh_key_from_agent(username)
                }
            } else if types.is_user_pass_plaintext() {
                Cred::credential_helper(&git_config, url, username)
            } else {
                Err(git2::Error::from_str(&format!(
                    "no supported auth methods available: {types:?}"
                )))
            }
        })
        .push_update_reference(|name, status| {
            debug!("update reference: name: {name} status: {status:?}");
            if let Some(msg) = status {
                Err(git2::Error::from_str(msg))
            } else {
                Ok(())
            }
        })
        .update_tips(|name, old, new| {
            debug!("update tip: name: {name} old: {old:?} new: {new:?}");
            true
        });

        let mut opts = PushOptions::new();
        opts.remote_callbacks(cbs);

        let mut remote = self.repo.find_remote("origin")?;

        let head_ref = self.repo.head()?.resolve()?;
        let name = head_ref
            .name()
            .ok_or_else(|| Error::Generic(format!("unable to resolve HEAD")))?;

        Ok(remote.push(&[name], Some(&mut opts))?)
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

        let message = if force {
            format!("update {host} (forced)")
        } else {
            format!("update {host}")
        };

        if let Some(commit) = self.commit(&message, force)? {
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
