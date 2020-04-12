use std::fmt::{Display, Formatter};

use clap::{app_from_crate, crate_authors, crate_description, crate_name, crate_version, Arg};
use reqwest::{Client, StatusCode, Url};

#[derive(Debug)]
enum Error {
    Http(StatusCode),
    Reqwest(reqwest::Error),
    Url(url::ParseError),
}

impl Display for Error {
    fn fmt(&self, formatter: &mut Formatter) -> std::fmt::Result {
        match self {
            Error::Http(status) => write!(formatter, "{}", status),
            Error::Reqwest(e) => write!(formatter, "{}", e),
            Error::Url(e) => write!(formatter, "{}", e),
        }
    }
}

impl std::error::Error for Error {}

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

#[tokio::main]
async fn main() -> Result<(), Error> {
    let args = app_from_crate!()
        .arg(Arg::with_name("contest id").index(1).required(true))
        .get_matches();
    let contest_id = args.value_of("contest id").unwrap();
    println!("contest id: {}", contest_id);

    let root_url = Url::parse("https://atcoder.jp")?;
    let login_url = root_url.join("login")?;
    let client = Client::builder().cookie_store(true).build()?;
    let response = client.get(login_url).send().await?;
    if response.status() != StatusCode::OK {
        return Err(Error::Http(response.status()));
    }
    response.headers().iter().for_each(|(key, value)| {
        println!("{:?}: {:?}", key, value);
    });
    response.cookies().for_each(|cookie| {
        println!("{:?}", cookie);
    });

    Ok(())
}
