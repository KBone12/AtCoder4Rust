use std::{
    collections::HashMap,
    env,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::Path,
};

use clap::{app_from_crate, crate_authors, crate_description, crate_name, crate_version, Arg};
use futures::future::join_all;
use percent_encoding;
use reqwest::{
    header::{self, HeaderMap, HeaderValue},
    Client, Response, StatusCode, Url,
};
use scraper::{Html, Selector};

mod error;
use error::Error;

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

fn parse_samples(text: &str) -> Result<Vec<(String, String)>, Error> {
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

async fn get_samples(
    text: &str,
    client: &Client,
    root_url: &Url,
    cookies: &HeaderMap,
) -> Result<HashMap<String, Vec<(String, String)>>, Error> {
    let document = Html::parse_document(text);
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
                parse_samples(&text).and_then(|samples| Ok((task_name, samples)))
            }
        });
    join_all(samples).await.into_iter().collect()
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let args = app_from_crate!()
        .arg(Arg::with_name("contest id").required(true))
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
        .arg(
            Arg::with_name("cookie")
                .short("c")
                .takes_value(true)
                .help("Path to the cookie directory (default: current directory)"),
        )
        .arg(
            Arg::with_name("root")
                .short("r")
                .takes_value(true)
                .help("Project root (default: current directory)"),
        )
        .get_matches();
    let contest_id = args.value_of("contest id").unwrap();
    let username = args.value_of("user").unwrap();
    let password = args.value_of("password").unwrap();

    let root_url = Url::parse("https://atcoder.jp/")?;
    let client = Client::builder().cookie_store(true).build()?;
    let cookies: Option<HeaderMap> = {
        let cookie_path = if let Some(path) = args.value_of("cookie") {
            Path::new(path).to_owned()
        } else {
            env::current_dir().unwrap()
        };
        if !cookie_path.exists() {
            fs::create_dir_all(cookie_path.clone())?;
        }
        let cookie_path = cookie_path.join("cookie.txt");
        if cookie_path.exists() {
            let reader = BufReader::new(File::open(cookie_path)?);
            Some(
                reader
                    .lines()
                    .filter_map(|line| line.ok())
                    .filter_map(|line| HeaderValue::from_str(&line).ok())
                    .map(|value| (header::COOKIE, value))
                    .collect(),
            )
        } else {
            None
        }
    };
    let cookies = if let Some(cookies) = cookies {
        cookies
    } else {
        let login_url = root_url.join("login")?;
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

        let cookie_path = if let Some(path) = args.value_of("cookie") {
            Path::new(path).to_owned()
        } else {
            env::current_dir().unwrap()
        };
        if !cookie_path.exists() {
            fs::create_dir_all(cookie_path.clone())?;
        }
        let cookie_path = cookie_path.join("cookie.txt");
        OpenOptions::new()
            .write(true)
            .create(true)
            .open(cookie_path)?
            .write_all(
                cookies
                    .iter()
                    .flat_map(|(_, value)| value.to_str().ok())
                    .collect::<Vec<_>>()
                    .join("\n")
                    .as_bytes(),
            )?;

        cookies
    };
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
    let samples = get_samples(&html, &client, &root_url, &cookies).await?;

    let root_path = if let Some(root_path) = args.value_of("root") {
        Path::new(root_path).to_owned()
    } else {
        env::current_dir().unwrap()
    }
    .join(contest_id);
    if root_path.exists() {
        return Err(Error::Invalid(format!("{} is already exists", contest_id)));
    }
    fs::create_dir(root_path.clone())?;
    OpenOptions::new()
        .write(true)
        .create(true)
        .open(root_path.join("Cargo.toml"))?
        .write_all(
            format!(
                r#"[package]
name = "{contest_id}"
version = "0.0.0"
authors = ["{username}"]
edition = "2018"

[dependencies]
"#,
                contest_id = contest_id,
                username = username
            )
            .as_bytes(),
        )?;
    let src_path = root_path.join("src");
    let sample_keys = {
        let mut tmp = samples.keys().collect::<Vec<_>>();
        tmp.sort();
        tmp
    };
    fs::create_dir(src_path.clone())?;
    OpenOptions::new()
        .write(true)
        .create(true)
        .open(src_path.join("main.rs"))?
        .write_all(
            format!(
                r#"{mods}
fn main() {{
    let args = std::env::args();
    if args.len() < 2 {{
        return;
    }}
    match args.nth(1) {{
{matches}
    }}
}}
"#,
                mods = sample_keys
                    .iter()
                    .map(|key| format!("mod {};\n", key.to_lowercase()))
                    .collect::<String>(),
                matches = sample_keys
                    .iter()
                    .map(|key| {
                        format!(
                            r#"        "{key}" => {key}::main(),"#,
                            key = key.to_lowercase()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
            .as_bytes(),
        )?;
    samples
        .iter()
        .map(|(key, samples)| {
            let test_cases = samples
                .iter()
                .enumerate()
                .map(|(index, (input, output))| {
                    format!(
                        r##"    #[test]
    fn sample_{index}() {{
        let test_dir = TestDir::new("./main {key}", "");
        let output = test_dir
            .cmd()
            .output_with_stdin(r#"{input}"#)
            .tee_output()
            .expect_success();
        assert_eq!(output.stdout_str(), r#"{output}"#);
        assert!(output.stderr_str().is_empty(), "stderr is not empty");
    }}
"##,
                        index = index,
                        key = key.to_lowercase(),
                        input = input,
                        output = output
                    )
                })
                .collect::<String>();
            OpenOptions::new()
                .write(true)
                .create(true)
                .open(src_path.join(key.to_lowercase() + ".rs"))
                .and_then(|mut options| {
                    options.write_all(
                        format!(
                            r#"use proconio::input;

pub fn main() {{
}}

#[cfg(test)]
mod tests {{
    use super::*;
    use cli_test_dir::*;
{test_cases}
}}
"#,
                            test_cases = test_cases
                        )
                        .as_bytes(),
                    )
                })
        })
        .collect::<Result<_, _>>()?;

    Ok(())
}
