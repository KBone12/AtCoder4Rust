use std::fmt::{Display, Formatter};

use clap::{app_from_crate, crate_authors, crate_description, crate_name, crate_version, Arg};
use percent_encoding;
use reqwest::{
    header::{self, HeaderMap},
    Client, Response, StatusCode, Url,
};
use scraper::{Html, Selector};

#[derive(Debug)]
enum Error {
    Http(StatusCode),
    Reqwest(reqwest::Error),
    Url(url::ParseError),
    Invalid(String),
}

impl Display for Error {
    fn fmt(&self, formatter: &mut Formatter) -> std::fmt::Result {
        match self {
            Error::Http(status) => write!(formatter, "{}", status),
            Error::Reqwest(e) => write!(formatter, "{}", e),
            Error::Url(e) => write!(formatter, "{}", e),
            Error::Invalid(msg) => write!(formatter, "Invalid: {}", msg),
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

fn get_csrf_token(response: &Response) -> Result<String, Error> {
    response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .filter(|value| value.starts_with("REVEL_SESSION"))
        .flat_map(|value| {
            value
                .split("%00")
                .filter(|value| value.starts_with("csrf_token"))
        })
        .map(percent_encoding::percent_decode_str)
        .map(|decoded| decoded.decode_utf8_lossy())
        .filter_map(|token| {
            token
                .split(":")
                .nth(1)
                .and_then(|token| Some(token.to_string()))
        })
        .next()
        .ok_or(Error::Invalid("Could not find csrf_token".to_string()))
}

fn get_cookies(response: &Response) -> HeaderMap {
    response
        .cookies()
        .map(|cookie| {
            (
                header::COOKIE,
                format!("{}={}", cookie.name(), cookie.value())
                    .parse()
                    .unwrap(),
            )
        })
        .collect()
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let args = app_from_crate!()
        .arg(Arg::with_name("contest id").index(1).required(true))
        .arg(
            Arg::with_name("user")
                .short("u")
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::with_name("password")
                .short("p")
                .takes_value(true)
                .required(true),
        )
        .get_matches();
    let contest_id = args.value_of("contest id").unwrap();
    let username = args.value_of("user").unwrap();
    let password = args.value_of("password").unwrap();

    let root_url = Url::parse("https://atcoder.jp/")?;
    let login_url = root_url.join("login")?;
    let client = Client::builder().cookie_store(true).build()?;
    let response = client.get(login_url).send().await?;
    if response.status() != StatusCode::OK {
        return Err(Error::Http(response.status()));
    }
    let csrf_token = get_csrf_token(&response)?;
    let login_url = root_url.join("login")?;
    let response = client
        .post(login_url)
        .headers(get_cookies(&response))
        .form(&[
            ("username", username),
            ("password", password),
            ("csrf_token", &csrf_token),
        ])
        .send()
        .await?;
    if response.status() != StatusCode::OK {
        return Err(Error::Http(response.status()));
    }
    let cookies = get_cookies(&response);
    let contest_url = root_url
        .join("contests/")?
        .join(&format!("{}/", contest_id))?
        .join("tasks")?;
    let response = client.get(contest_url).headers(cookies).send().await?;
    if response.status() != StatusCode::OK {
        return Err(Error::Http(response.status()));
    }
    let html = response.text().await?;
    let document = Html::parse_document(&html);
    document
        .select(&Selector::parse("tbody > tr").unwrap())
        .filter_map(|tr| tr.select(&Selector::parse("td a").unwrap()).next())
        .for_each(|a| println!("{}: {:?}", a.inner_html(), a.value().attr("href")));

    Ok(())
}
