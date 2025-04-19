use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

use git2::{
    Commit, Cred, ErrorCode, FetchOptions, IndexAddOption, Oid, PushOptions, Rebase, RebaseOptions,
    RemoteCallbacks, Repository, build::RepoBuilder,
};
use log::{debug, error};

use crate::{
    config::Config,
    error::{Error, Result},
};

use super::Syncer;

pub struct Git {
    path: PathBuf,
    cfg: Config,
    repo: Repository,
}

impl Git {
    pub fn existing(cfg: &Config) -> Result<Self> {
        let path = cfg.sync_path();
        let repo = Repository::open(&path)?;
        fs::create_dir_all(Path::new(&path).join("hosts"))?;
        Ok(Self {
            path,
            cfg: cfg.clone(),
            repo,
        })
    }

    pub fn new(cfg: &Config) -> Result<Self> {
        let git_config = git2::Config::open_default()?;

        // TODO(jp3): refactor the credentials code, and stop copy/pasting it
        // all over the place ...
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
                if !cfg.sync.ssh_key.is_empty() {
                    let privatekey = Path::new(&cfg.sync.ssh_key);
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
        });

        let mut opts = FetchOptions::new();
        opts.remote_callbacks(cbs);

        let mut builder = RepoBuilder::new();
        builder.fetch_options(opts);

        let path = cfg.sync_path();
        let repo = builder.clone(&cfg.sync.url, &path)?;
        fs::create_dir_all(Path::new(&path).join("hosts"))?;
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
            debug!("rebase failed");
            let index = self.repo.index()?;
            debug!("check for conflicts ...");
            for conflict in index.conflicts()? {
                let conflict = conflict?;
                debug!(
                    "{:?} -> {:?} / {:?}",
                    conflict.ancestor, conflict.our, conflict.their
                );
            }
            debug!("look at all files ...");
            for entry in index.iter() {
                debug!("{entry:?}");
            }
            // TODO(jp3): what do we do if abort fails? we are already handling
            // an error ...
            debug!("abort rebase");
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
            // let index = self.repo.index()?;
            // debug!(
            //     "INDEX: len: {}, is_empty: {}, has conflicts: {}",
            //     index.len(),
            //     index.is_empty(),
            //     index.has_conflicts()
            // );
            // debug!("look at all files ...");
            // for entry in index.iter() {
            //     debug!("{entry:?}");
            // }
            match rebase.commit(None, &committer, None) {
                Ok(oid) => debug!("updated {} -> {}", operation.id(), oid),
                Err(e) => {
                    if e.code() == ErrorCode::Applied {
                        debug!("patch already applied");
                    } else {
                        error!("commit failed: {e}");
                        return Err(Error::Git(e));
                    }
                }
            };
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
    fn refresh(&self) -> Result<PathBuf> {
        self.pull()?;
        Ok(Path::new(&self.path).join("hosts"))
    }

    fn push_changes(&self, host: &str, force: bool) -> Result<()> {
        let mut index = self.repo.index()?;

        // TODO(jp3): This should only be adding paths for the host being
        // updated, use a callback to do the filtering?
        index.add_all(["*"].iter(), IndexAddOption::FORCE, None)?;
        index.write()?;

        let message = if force {
            format!("update {host} (forced)")
        } else {
            format!("update {host}")
        };

        if let Some(_) = self.commit(&message, force)? {
            self.push()?;
        }

        Ok(())
    }
}
