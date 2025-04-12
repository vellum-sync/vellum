use std::{fmt::Display, fs, io};
use toml;
use serde::Deserialize;

pub type Result = std::result::Result<Config, Error>;

#[derive(Deserialize, Debug)]
pub struct Config {}

impl Config {
    pub fn load<S: Into<String>>(config: Option<S>) -> Result {
        match config {
            Some(path) => Self::open(path.into()),
            None => Ok(Self::default()),
        }
    }

    fn open(path: String) -> Result {
        let data = fs::read_to_string(&path)?;

        Ok(toml::from_str(&data)?)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self{}
    }
}

#[derive(Debug)]
pub enum Error {
    Read(io::Error),
    Parse(toml::de::Error),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read(e) => write!(f, "READ ERROR: {e}"),
            Self::Parse(e) => write!(f, "PARSE ERROR: {e}"),
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
