use std::{
    fmt, fs,
    path::{Path, PathBuf},
    result, thread,
    time::{Duration, Instant},
};

use git2::{
    Commit, Cred, CredentialType, ErrorCode, FetchOptions, FetchPrune, Index, IndexAddOption, Oid,
    PushOptions, Rebase, RebaseOptions, RemoteCallbacks, Repository, build::RepoBuilder,
};
use humantime::format_duration;
use log::{debug, error};

use crate::{
    config::Config,
    error::{Error, Result},
};

use super::{LockedSyncer, Syncer};

const LOCK_REF: &str = "refs/tags/lock";

const MAX_LOCK_WAIT: Duration = Duration::from_secs(300);

pub struct Git {
    path: PathBuf,
    cfg: Config,
    repo: Repository,
}

impl Git {
    fn existing(cfg: &Config) -> Result<Self> {
        let path = cfg.sync_path();
        let repo = Repository::open(&path)?;
        fs::create_dir_all(Path::new(&path).join("hosts"))?;
        Ok(Self {
            path,
            cfg: cfg.clone(),
            repo,
        })
    }

    fn clone(cfg: &Config) -> Result<Self> {
        let cm = CredsManager::new(cfg)?;

        let mut cbs = RemoteCallbacks::new();
        cbs.credentials(|url, username, types| cm.lookup(url, username, types));

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

    pub fn new(cfg: &Config) -> Result<Self> {
        if fs::exists(cfg.sync_path())? {
            Self::existing(cfg)
        } else {
            Self::clone(cfg)
        }
    }

    fn try_fetch(&self, mut locked: bool, mut changes: Option<Oid>) -> Result<(bool, Option<Oid>)> {
        let head_ref = self.repo.head()?;
        let ref_name = head_ref
            .name()
            .ok_or_else(|| Error::from_str("unable to resolve HEAD"))?;

        let upstream_ref_name = self.get_head_upstream_ref()?;

        let cm = CredsManager::new(&self.cfg)?;

        let mut cbs = RemoteCallbacks::new();
        cbs.credentials(|url, username, types| cm.lookup(url, username, types))
            .update_tips(|name, old, new| {
                debug!("update tip: name: {name} old: {old:?} new: {new:?}");
                debug!(
                    "name: {name}, ref_name: {ref_name}, upstream_ref_name: {upstream_ref_name}"
                );
                if name == upstream_ref_name && changes.is_none() {
                    changes = Some(old);
                }
                if name == LOCK_REF {
                    locked = !new.is_zero();
                    debug!("repo is locked: {locked}");
                }
                true
            });

        let mut opts = FetchOptions::new();
        opts.remote_callbacks(cbs).prune(FetchPrune::On);

        let mut remote = self.repo.find_remote("origin")?;

        remote.fetch::<&str>(
            &[ref_name, "refs/tags/*:refs/tags/*"],
            Some(&mut opts),
            None,
        )?;

        // make sure that the update_tips callback is gone, since it implicitly
        // borrows locked/changes.
        drop(opts);

        Ok((locked, changes))
    }

    fn fetch(&self) -> Result<Option<Oid>> {
        debug!("start fetch ...");

        let (mut locked, mut changes) = self.try_fetch(false, None)?;

        let start = Instant::now();
        while locked && start.elapsed() < MAX_LOCK_WAIT {
            debug!("waiting for repo to unlock ...");
            thread::sleep(Duration::from_secs(1));
            (locked, changes) = self.try_fetch(locked, changes)?;
        }

        if locked {
            return Err(Error::Generic(format!(
                "repo did not unlock within {}",
                format_duration(start.elapsed())
            )));
        }

        debug!("fetch has changes: {changes:?}");

        Ok(changes)
    }

    fn rebase(&self, old: Option<Oid>) -> Result<()> {
        debug!("start rebase (old: {old:?})");

        let head = self.repo.head()?;
        let upstream_ref_name = self.get_head_upstream_ref()?;
        let upstream_ref = self.repo.find_reference(&upstream_ref_name)?;

        let branch = self.repo.reference_to_annotated_commit(&head)?;
        let upstream = match old {
            Some(old) => self.repo.find_annotated_commit(old)?,
            None => self.repo.reference_to_annotated_commit(&upstream_ref)?,
        };
        let onto = self.repo.reference_to_annotated_commit(&upstream_ref)?;

        debug!(
            "start rebase of {:?} onto {:?}",
            branch.refname(),
            onto.refname()
        );

        let mut opts = RebaseOptions::new();
        opts.inmemory(false);

        let mut rebase =
            self.repo
                .rebase(Some(&branch), Some(&upstream), Some(&onto), Some(&mut opts))?;

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
        }
    }

    fn pull(&self) -> Result<()> {
        if let Some(old) = self.fetch()? {
            self.rebase(Some(old))?;
        }
        Ok(())
    }

    fn locked_pull(&self) -> Result<()> {
        if let (_, Some(old)) = self.try_fetch(false, None)? {
            self.rebase(Some(old))?;
        }
        Ok(())
    }

    fn push(&self) -> Result<()> {
        match self.try_push() {
            Err(Error::Git(e)) => {
                if e.code() == ErrorCode::NotFastForward {
                    debug!("push failed due to NotFasForward, try pull ...");
                    self.pull()?;
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

        let cm = CredsManager::new(&self.cfg)?;

        let mut cbs = RemoteCallbacks::new();
        cbs.credentials(|url, username, types| cm.lookup(url, username, types))
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
            .ok_or_else(|| Error::from_str("unable to resolve HEAD"))?;

        Ok(remote.push(&[name], Some(&mut opts))?)
    }

    fn force_push(&self) -> Result<()> {
        debug!("start force push ...");

        let head_ref = self.repo.head()?;
        let name = head_ref
            .name()
            .ok_or_else(|| Error::from_str("unable to resolve HEAD"))?;

        let remote_target = self.get_head_upstream_target()?;
        debug!("remote target: {}", remote_target);

        let cm = CredsManager::new(&self.cfg)?;

        let mut cbs = RemoteCallbacks::new();
        cbs.credentials(|url, username, types| cm.lookup(url, username, types))
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
            })
            .push_negotiation(|updates| {
                for update in updates {
                    debug!(
                        "update {:?} = {} -> {:?} = {}",
                        update.src_refname(),
                        update.src(),
                        update.dst_refname(),
                        update.dst()
                    );
                    if let Some(src_ref) = update.src_refname() {
                        if src_ref == name && update.src() != remote_target {
                            return Err(git2::Error::from_str("remote oid has changed"));
                        }
                    }
                }
                Ok(())
            });

        let mut opts = PushOptions::new();
        opts.remote_callbacks(cbs);

        let refspec = format!("+{name}:{name}");
        let mut remote = self.repo.find_remote("origin")?;

        Ok(remote.push(&[&refspec], Some(&mut opts))?)
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

    fn commit_no_parent(&self, message: &str) -> Result<Oid> {
        let mut index = self.repo.index()?;
        let tree = self.repo.find_tree(index.write_tree()?)?;
        let author = self.repo.signature()?;
        let commit = self
            .repo
            .commit(None, &author, &author, message, &tree, &[])?;

        debug!("Created commit {commit:?}");

        let head_ref = self.repo.head()?;
        let ref_name = head_ref
            .name()
            .ok_or_else(|| Error::from_str("unable to resolve HEAD"))?;

        let ref_msg = "rebuild history";

        let new_ref = match head_ref.target() {
            Some(current) => self
                .repo
                .reference_matching(ref_name, commit, true, current, ref_msg)?,
            None => self.repo.reference(ref_name, commit, true, ref_msg)?,
        };
        debug!(
            "updated reference: {:?} -> {:?}",
            new_ref.name(),
            new_ref.target()
        );

        Ok(commit)
    }

    fn unlock(&self) -> Result<()> {
        let cm = CredsManager::new(&self.cfg)?;

        let mut cbs = RemoteCallbacks::new();
        cbs.credentials(|url, username, types| cm.lookup(url, username, types))
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

        let refspec = format!(":{LOCK_REF}");

        remote.push(&[&refspec], Some(&mut opts))?;

        Ok(())
    }

    fn get_head_upstream_ref(&self) -> Result<String> {
        let head = self.repo.head()?;
        let name = head
            .name()
            .ok_or_else(|| Error::Generic("failed to get HEAD".to_string()))?;

        let remote_ref_name = self.repo.branch_upstream_name(name)?;

        Ok(remote_ref_name
            .as_str()
            .ok_or_else(|| Error::Generic("failed to get remote_ref".to_string()))?
            .to_string())
    }

    fn get_head_upstream_target(&self) -> Result<Oid> {
        let remote_ref_name = self.get_head_upstream_ref()?;
        let remote_ref = self.repo.find_reference(&remote_ref_name)?;
        remote_ref
            .target()
            .ok_or_else(|| Error::Generic("failed to get remote target".to_string()))
    }

    fn unpushed_changes(&self) -> Result<usize> {
        let upstream = self.get_head_upstream_target()?;

        let mut walk = self.repo.revwalk()?;
        walk.push_head()?;
        walk.hide(upstream)?;

        Ok(walk.count())
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

        self.commit(&message, force)?;

        let changes = self.unpushed_changes()?;
        debug!("unpushed changes: {changes}");
        if changes > 0 || force {
            self.push()?;
        }

        Ok(())
    }

    fn lock<'a>(&'a self) -> Result<Box<dyn LockedSyncer + 'a>> {
        let mut index = Index::new()?;
        let oid = index.write_tree_to(&self.repo)?;
        let tree = self.repo.find_tree(oid)?;

        debug!("lock tree: {tree:?}");

        let message = format!("lock for {}", self.cfg.hostname.to_string_lossy());

        let author = self.repo.signature()?;
        let commit = self
            .repo
            .commit(None, &author, &author, &message, &tree, &[])?;
        debug!("Created commit {commit:?}");

        let cm = CredsManager::new(&self.cfg)?;

        let mut cbs = RemoteCallbacks::new();
        cbs.credentials(|url, username, types| cm.lookup(url, username, types))
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

        let refspec = format!("{commit:?}:{LOCK_REF}");

        remote.push(&[&refspec], Some(&mut opts))?;

        Ok(Box::new(GitGuard::new(self)))
    }
}

struct CredsManager {
    cfg: Config,
    git_config: git2::Config,
}

impl CredsManager {
    fn new(cfg: &Config) -> Result<Self> {
        let git_config = git2::Config::open_default()?;
        Ok(Self {
            cfg: cfg.clone(),
            git_config,
        })
    }

    fn lookup(
        &self,
        url: &str,
        username: Option<&str>,
        types: CredentialType,
    ) -> result::Result<git2::Cred, git2::Error> {
        debug!("trying to find credentials for {url} (username: {username:?}, types: {types:?})");
        if types.is_default() {
            Cred::default()
        } else if types.is_ssh_key() {
            let username =
                username.ok_or_else(|| git2::Error::from_str("missing username for ssh auth"))?;
            if !self.cfg.sync.ssh_key.is_empty() {
                let privatekey = Path::new(&self.cfg.sync.ssh_key);
                Cred::ssh_key(username, None, privatekey, None)
            } else {
                Cred::ssh_key_from_agent(username)
            }
        } else if types.is_user_pass_plaintext() {
            Cred::credential_helper(&self.git_config, url, username)
        } else {
            Err(git2::Error::from_str(&format!(
                "no supported auth methods available: {types:?}"
            )))
        }
    }
}

#[derive(Debug)]
struct GitGuard<'a> {
    git: &'a Git,
}

impl<'a> GitGuard<'a> {
    fn new(git: &'a Git) -> Self {
        Self { git }
    }
}

impl LockedSyncer for GitGuard<'_> {
    fn refresh(&self) -> Result<PathBuf> {
        self.git.locked_pull()?;
        Ok(Path::new(&self.git.path).join("hosts"))
    }

    fn push_changes(&self, host: &str) -> Result<()> {
        let mut index = self.git.repo.index()?;

        index.add_all(["*"].iter(), IndexAddOption::FORCE, None)?;
        index.write()?;

        let message = format!("rebuild full history from {host}");

        self.git.commit_no_parent(&message)?;
        self.git.force_push()?;

        Ok(())
    }

    fn unlock(&self) -> Result<()> {
        self.git.unlock()
    }
}
