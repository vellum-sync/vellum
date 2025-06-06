use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
    time::Duration,
};
use xdg::BaseDirectories;

pub type Result = crate::error::Result<Config>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    #[serde(skip)]
    pub path: Option<PathBuf>,

    #[serde(default = "default_cache_dir")]
    pub cache_dir: PathBuf,

    #[serde(default = "default_state_dir")]
    pub state_dir: PathBuf,

    #[serde(default = "default_hostname")]
    pub hostname: PathBuf,

    #[serde(default)]
    pub sync: Sync,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Sync {
    /// Controls if sync is enabled
    #[serde(default = "default_sync_enabled")]
    pub enabled: bool,

    /// URL of upstream git repository
    #[serde(default)]
    pub url: String,

    /// SSH private key file used for SSH git auth
    #[serde(default)]
    pub ssh_key: String,

    /// How often should we run an automatic sync?
    #[serde(default = "default_sync_interval")]
    #[serde(with = "humantime_serde")]
    pub interval: Duration,

    /// Path of the sync git checkout, non-absolute paths are relative to the
    /// state directory.
    #[serde(default = "default_sync_path")]
    path: PathBuf,
}

impl Config {
    pub fn load<S: Into<String>>(config: Option<S>) -> Result {
        match config {
            Some(path) => Self::open(path.into()),
            None => Self::open_default(),
        }
    }

    fn open<P: AsRef<Path>>(path: P) -> Result {
        let p = path.as_ref();

        let data = fs::read_to_string(p)?;

        let mut cfg: Config = toml::from_str(&data)?;
        cfg.path = Some(p.to_path_buf());

        Ok(cfg)
    }

    fn open_default() -> Result {
        if let Ok(path) = env::var("VELLUM_CONFIG") {
            if fs::exists(&path)? {
                return Self::open(path);
            }
            return Ok(Self::default());
        };

        let dirs = BaseDirectories::with_prefix("vellum")?;

        match dirs.find_config_file("config.toml") {
            Some(path) => Self::open(path),
            None => Ok(Self::default()),
        }
    }

    pub fn show(&self) -> crate::error::Result<()> {
        let cfg = toml::to_string_pretty(self)?;
        print!("{cfg}");
        Ok(())
    }

    pub fn sync_path(&self) -> PathBuf {
        Path::new(&self.state_dir).join(&self.sync.path)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            path: None,
            cache_dir: default_cache_dir(),
            state_dir: default_state_dir(),
            hostname: default_hostname(),
            sync: Sync::default(),
        }
    }
}

impl Default for Sync {
    fn default() -> Self {
        Self {
            enabled: default_sync_enabled(),
            url: "".to_string(),
            ssh_key: "".to_string(),
            interval: default_sync_interval(),
            path: default_sync_path(),
        }
    }
}

fn default_cache_dir() -> PathBuf {
    match BaseDirectories::with_prefix("vellum") {
        Ok(d) => d.get_cache_home(),
        Err(e) => panic!("failed to load XDG directories: {e}"),
    }
}

fn default_state_dir() -> PathBuf {
    match BaseDirectories::with_prefix("vellum") {
        Ok(d) => d.get_state_home(),
        Err(e) => panic!("failed to load XDG directories: {e}"),
    }
}

fn default_hostname() -> PathBuf {
    match hostname::get() {
        Ok(h) => h.into(),
        Err(e) => panic!("failed to get hostname: {e}"),
    }
}

fn default_sync_enabled() -> bool {
    true
}

fn default_sync_interval() -> Duration {
    Duration::from_secs(30)
}

fn default_sync_path() -> PathBuf {
    Path::new("sync").into()
}
