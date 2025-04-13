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

    #[serde(default)]
    pub state_dir: PathBuf,
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
        let dirs = match BaseDirectories::with_prefix("vellum") {
            Ok(d) => d,
            Err(e) => panic!("failed to load XDG directories: {e}"),
        };
        Self {
            path: None,
            state_dir: dirs.get_state_home(),
        }
    }
}
