use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};
use toml;
use xdg::BaseDirectories;

use crate::error::Error;

pub type Result = crate::error::Result<Config>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    #[serde(skip)]
    pub path: Option<PathBuf>,

    #[serde(default = "default_state_dir")]
    pub state_dir: PathBuf,

    #[serde(default = "default_hostname")]
    pub hostname: PathBuf,
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
        let dirs = BaseDirectories::with_prefix("vellum")?;

        match dirs.find_config_file("config.toml") {
            Some(path) => Self::open(path),
            None => Ok(Self::default()),
        }
    }

    pub fn show(&self) -> std::result::Result<(), Error> {
        let cfg = toml::to_string(self)?;
        print!("{cfg}");
        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            path: None,
            state_dir: default_state_dir(),
            hostname: default_hostname(),
        }
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
