use std::{
    collections::HashMap,
    fmt::{Display, Formatter},
};

use clap::{app_from_crate, crate_authors, crate_description, crate_name, crate_version, Arg};
use futures::future::join_all;
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

fn get_samples(text: &str) -> Result<Vec<(String, String)>, Error> {
    let document = Html::parse_document(&text);
    let (inputs, outputs): (Vec<_>, Vec<_>) = document
        .select(&Selector::parse("#task-statement .part").unwrap())
        .filter_map(|part| {
            part.select(&Selector::parse("h3").unwrap())
                .filter_map(|h3| {
                    if let Some(text) = h3.text().find(|text| text.starts_with("入力例")) {
                        text.split_whitespace()
                            .nth(1)
                            .and_then(|index| Some((part, index, true)))
                    } else if let Some(text) = h3.text().find(|text| text.starts_with("出力例"))
                    {
                        text.split_whitespace()
                            .nth(1)
                            .and_then(|index| Some((part, index, false)))
                    } else {
                        None
                    }
                })
                .next()
        })
        .filter_map(|(part, index, is_input)| {
            part.select(&Selector::parse("pre").unwrap())
                .map(|pre| (pre.inner_html(), index, is_input))
                .next()
        })
        .partition(|(_, _, is_input)| *is_input);
    Ok(inputs
        .iter()
        .map(|(input, _, _)| input.clone())
        .zip(outputs.iter().map(|(output, _, _)| output.clone()))
        .collect())
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
    let response = client
        .get(contest_url)
        .headers(cookies.clone())
        .send()
        .await?;
    if response.status() != StatusCode::OK {
        return Err(Error::Http(response.status()));
    }
    let html = response.text().await?;
    let document = Html::parse_document(&html);
    let selector = Selector::parse("tbody > tr").unwrap();
    let samples = document
        .select(&selector)
        .filter_map(|tr| tr.select(&Selector::parse("td a").unwrap()).next())
        .map(|a| {
            let task_name = a.inner_html();
            let url = a.value().attr("href").unwrap();
            let root_url = root_url.clone();
            let client = client.clone();
            let cookies = cookies.clone();
            async move {
                let response = client
                    .get(root_url.join(url)?)
                    .headers(cookies)
                    .send()
                    .await?;
                let text = response.text().await?;
                get_samples(&text).and_then(|samples| Ok((task_name, samples)))
            }
        });
    let samples = join_all(samples)
        .await
        .into_iter()
        .collect::<Result<HashMap<_, _>, Error>>()?;
    println!("{:?}", samples);

    Ok(())
}
