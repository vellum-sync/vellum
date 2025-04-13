use std::{error, fmt::Display, io, result};

use xdg::BaseDirectoriesError;

#[derive(Debug)]
pub enum Error {
    Daemon(i32),
    IO(io::Error),
    Encoding(serde_json::Error),
    Parse(toml::de::Error),
    Format(toml::ser::Error),
    Lookup(BaseDirectoriesError),
    Generic(String),
    Git(git2::Error),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Daemon(errno) => write!(f, "failed to start server daemon (errno={errno})"),
            Self::IO(e) => write!(f, "IO ERROR: {e}"),
            Self::Encoding(e) => write!(f, "ENCODING ERROR: {e}"),
            Self::Parse(e) => write!(f, "PARSE ERROR: {e}"),
            Self::Format(e) => write!(f, "FORMAT ERROR: {e}"),
            Self::Lookup(e) => write!(f, "LOOKUP ERROR: {e}"),
            Self::Generic(s) => write!(f, "{s}"),
            Self::Git(e) => write!(f, "GIT ERROR: {e}"),
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::Daemon(_) => None,
            Self::IO(e) => Some(e),
            Self::Encoding(e) => Some(e),
            Self::Parse(e) => Some(e),
            Self::Format(e) => Some(e),
            Self::Lookup(e) => Some(e),
            Self::Generic(_) => None,
            Self::Git(e) => Some(e),
        }
    }
}

impl From<i32> for Error {
    fn from(value: i32) -> Self {
        Self::Daemon(value)
    }
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::IO(value)
    }
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Self::Encoding(value)
    }
}

impl From<toml::de::Error> for Error {
    fn from(value: toml::de::Error) -> Self {
        Self::Parse(value)
    }
}

impl From<toml::ser::Error> for Error {
    fn from(value: toml::ser::Error) -> Self {
        Self::Format(value)
    }
}

impl From<BaseDirectoriesError> for Error {
    fn from(value: BaseDirectoriesError) -> Self {
        Self::Lookup(value)
    }
}

impl From<git2::Error> for Error {
    fn from(value: git2::Error) -> Self {
        Self::Git(value)
    }
}

pub type Result<T> = result::Result<T, Error>;
