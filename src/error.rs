use std::{
    env::VarError,
    error,
    fmt::{Debug, Display},
    io,
    num::ParseIntError,
    result,
    sync::mpsc::SendError,
};

use aws_lc_rs::error::{KeyRejected, Unspecified};
use base64::DecodeError;
use xdg::BaseDirectoriesError;

#[derive(Debug)]
pub enum Error {
    Daemon(i32),
    IO(io::Error),
    Encoding(serde_json::Error),
    Encode(rmp_serde::encode::Error),
    Decode(rmp_serde::decode::Error),
    Parse(toml::de::Error),
    Format(toml::ser::Error),
    Lookup(BaseDirectoriesError),
    Generic(String),
    CryptKey(KeyRejected),
    Crypt,
    Git(git2::Error),
    Base64(DecodeError),
    EnvVar(VarError),
    UUID(uuid::Error),
    ParseInt(ParseIntError),
    ParseTime(chrono::ParseError),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Daemon(errno) => write!(f, "failed to start server daemon (errno={errno})"),
            Self::IO(e) => write!(f, "IO ERROR: {e}"),
            Self::Encoding(e) => write!(f, "ENCODING ERROR: {e}"),
            Self::Encode(e) => write!(f, "ENCODE ERROR: {e}"),
            Self::Decode(e) => write!(f, "DECODE ERROR: {e}"),
            Self::Parse(e) => write!(f, "PARSE ERROR: {e}"),
            Self::Format(e) => write!(f, "FORMAT ERROR: {e}"),
            Self::Lookup(e) => write!(f, "LOOKUP ERROR: {e}"),
            Self::Generic(s) => write!(f, "{s}"),
            Self::CryptKey(e) => write!(f, "CRYPT KEY ERROR: {e}"),
            Self::Crypt => write!(f, "CRYPT ERROR"),
            Self::Git(e) => write!(f, "GIT ERROR: {e}"),
            Self::Base64(e) => write!(f, "BASE64 DECODE ERROR: {e}"),
            Self::EnvVar(e) => write!(f, "ENVIRONMENT VARIABLE ERROR: {e}"),
            Self::UUID(e) => write!(f, "UUID ERROR: {e}"),
            Self::ParseInt(e) => write!(f, "PARSE INT ERROR: {e}"),
            Self::ParseTime(e) => write!(f, "PARSE TIME ERROR: {e}"),
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::Daemon(_) => None,
            Self::IO(e) => Some(e),
            Self::Encoding(e) => Some(e),
            Self::Encode(e) => Some(e),
            Self::Decode(e) => Some(e),
            Self::Parse(e) => Some(e),
            Self::Format(e) => Some(e),
            Self::Lookup(e) => Some(e),
            Self::Generic(_) => None,
            Self::CryptKey(e) => Some(e),
            Self::Crypt => None,
            Self::Git(e) => Some(e),
            Self::Base64(e) => Some(e),
            Self::EnvVar(e) => Some(e),
            Self::UUID(e) => Some(e),
            Self::ParseInt(e) => Some(e),
            Self::ParseTime(e) => Some(e),
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

impl From<rmp_serde::encode::Error> for Error {
    fn from(value: rmp_serde::encode::Error) -> Self {
        Self::Encode(value)
    }
}

impl From<rmp_serde::decode::Error> for Error {
    fn from(value: rmp_serde::decode::Error) -> Self {
        Self::Decode(value)
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

impl From<KeyRejected> for Error {
    fn from(value: KeyRejected) -> Self {
        Self::CryptKey(value)
    }
}

impl From<Unspecified> for Error {
    fn from(_: Unspecified) -> Self {
        Self::Crypt
    }
}

impl From<DecodeError> for Error {
    fn from(value: DecodeError) -> Self {
        Self::Base64(value)
    }
}

impl From<VarError> for Error {
    fn from(value: VarError) -> Self {
        Self::EnvVar(value)
    }
}

impl From<uuid::Error> for Error {
    fn from(value: uuid::Error) -> Self {
        Self::UUID(value)
    }
}

impl From<ParseIntError> for Error {
    fn from(value: ParseIntError) -> Self {
        Self::ParseInt(value)
    }
}

impl From<chrono::ParseError> for Error {
    fn from(value: chrono::ParseError) -> Self {
        Self::ParseTime(value)
    }
}

impl<T: Debug> From<SendError<T>> for Error {
    fn from(value: SendError<T>) -> Self {
        Self::Generic(format!("failed to send data: {value}"))
    }
}

pub type Result<T> = result::Result<T, Error>;
