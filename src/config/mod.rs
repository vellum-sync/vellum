use serde::Deserialize;
use std::{
    fmt::Display,
    fs, io,
    path::{Path, PathBuf},
};
use toml;
use xdg::{BaseDirectories, BaseDirectoriesError};

pub type Result = std::result::Result<Config, Error>;

#[derive(Deserialize, Debug)]
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

#[derive(Debug)]
pub enum Error {
    Read(io::Error),
    Parse(toml::de::Error),
    Lookup(BaseDirectoriesError),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read(e) => write!(f, "READ ERROR: {e}"),
            Self::Parse(e) => write!(f, "PARSE ERROR: {e}"),
            Self::Lookup(e) => write!(f, "LOOKUP ERROR: {e}"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Read(value)
    }
}

impl From<toml::de::Error> for Error {
    fn from(value: toml::de::Error) -> Self {
        Self::Parse(value)
    }
}

impl From<BaseDirectoriesError> for Error {
    fn from(value: BaseDirectoriesError) -> Self {
        Self::Lookup(value)
    }
}
