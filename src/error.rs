use std::fmt::{Display, Formatter};

use reqwest::StatusCode;

#[derive(Debug)]
pub enum Error {
    Http(StatusCode),
    Io(std::io::Error),
    Reqwest(reqwest::Error),
    Url(url::ParseError),
    Invalid(String),
}

impl Display for Error {
    fn fmt(&self, formatter: &mut Formatter) -> std::fmt::Result {
        match self {
            Error::Http(status) => write!(formatter, "{}", status),
            Error::Io(e) => write!(formatter, "{}", e),
            Error::Reqwest(e) => write!(formatter, "{}", e),
            Error::Url(e) => write!(formatter, "{}", e),
            Error::Invalid(msg) => write!(formatter, "Invalid: {}", msg),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Self {
        Self::Reqwest(error)
    }
}

impl From<url::ParseError> for Error {
    fn from(error: url::ParseError) -> Self {
        Self::Url(error)
    }
}
