use std::{
    collections::HashMap,
    env,
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, BufWriter, Read, Write},
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
mod generator;
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
    cookies: &Option<HeaderMap>,
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
                    .headers(cookies.unwrap_or_default())
                    .send()
                    .await?;
                let text = response.text().await?;
                parse_samples(&text).and_then(|samples| Ok((task_name, samples)))
            }
        });
    join_all(samples).await.into_iter().collect()
}

async fn login(
    url: Url,
    client: &Client,
    username: &str,
    password: &str,
) -> Result<HeaderMap, Error> {
    let response = client.get(url.clone()).send().await?;
    if response.status() != StatusCode::OK {
        return Err(Error::Http(response.status()));
    }
    let csrf_token = get_csrf_token(&response)?;
    let response = client
        .post(url)
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
    Ok(get_cookies(&response))
}

fn load_cookies<P: AsRef<Path>>(path: P) -> Result<HeaderMap, Error> {
    let reader = BufReader::new(File::open(path)?);
    Ok(reader
        .lines()
        .filter_map(|line| line.ok())
        .filter_map(|line| HeaderValue::from_str(&line).ok())
        .map(|value| (header::COOKIE, value))
        .collect())
}

fn save_cookies<P: AsRef<Path>>(cookies: &HeaderMap, path: P) -> Result<(), Error> {
    let mut writer = BufWriter::new(OpenOptions::new().write(true).create(true).open(path)?);
    writer.write_all(
        cookies
            .iter()
            .filter_map(|(_, value)| value.to_str().ok())
            .collect::<Vec<_>>()
            .join("\n")
            .as_bytes(),
    )?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let args = app_from_crate!()
        .arg(
            Arg::with_name("contest id")
                .required(true)
                .help("Contest's id (e.g. abc001)"),
        )
        .arg(Arg::with_name("user").short("u").takes_value(true))
        .arg(Arg::with_name("password").short("p").takes_value(true))
        .arg(
            Arg::with_name("cookie")
                .short("c")
                .takes_value(true)
                .help("Path to the cookie file (default: cookie.txt in the current directory)"),
        )
        .arg(Arg::with_name("no-login").long("no-login"))
        .arg(
            Arg::with_name("root")
                .short("r")
                .takes_value(true)
                .help("Project's root (default: current directory)"),
        )
        .arg(
            Arg::with_name("dependencies")
                .short("d")
                .takes_value(true)
                .help("Path to the file which is a dependency list written in Cargo.toml format"),
        )
        .arg(
            Arg::with_name("template")
                .short("t")
                .takes_value(true)
                .help("Path to the template file for [task].rs"),
        )
        .get_matches();
    let contest_id = args.value_of("contest id").unwrap();
    let username = args.value_of("user");
    let password = args.value_of("password");

    let root_url = Url::parse("https://atcoder.jp/")?;
    let client = Client::builder().cookie_store(true).build()?;
    let cookies: Option<HeaderMap> = {
        // Find a local cookie file
        let cookie_path = if let Some(path) = args.value_of("cookie") {
            Path::new(path).to_owned()
        } else {
            env::current_dir()?.join("cookie.txt")
        };
        if cookie_path.exists() {
            Some(load_cookies(cookie_path)?)
        } else {
            None
        }
    };
    let cookies = if args.is_present("no-login") {
        None
    } else if let Some(cookies) = cookies {
        Some(cookies)
    } else {
        // Login interactively & save cookies
        let username = if let Some(username) = username {
            username.to_owned()
        } else {
            print!("User name: ");
            io::stdout().flush()?;
            let mut buf = String::new();
            io::stdin().read_line(&mut buf)?;
            buf.trim().to_owned()
        };
        let password = if let Some(password) = password {
            password.to_owned()
        } else {
            print!("Password: ");
            io::stdout().flush()?;
            let mut buf = String::new();
            io::stdin().read_line(&mut buf)?;
            buf.trim().to_owned()
        };
        let cookies = login(root_url.join("login")?, &client, &username, &password).await?;
        let succeeded = cookies
            .get_all(header::COOKIE)
            .iter()
            .filter_map(|cookie| cookie.to_str().ok())
            .inspect(|cookie| println!("{}", cookie))
            .any(|cookie| cookie.contains(&username));
        if !succeeded {
            return Err(Error::Invalid("Failed to login".to_owned()));
        }

        let cookie_path = if let Some(path) = args.value_of("cookie") {
            let path = Path::new(path);
            let parent = path.parent().expect("--cookie must be a path to the file");
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
            path.to_owned()
        } else {
            env::current_dir().unwrap().join("cookie.txt")
        };
        save_cookies(&cookies, cookie_path)?;

        Some(cookies)
    };
    let contest_url = root_url
        .join("contests/")?
        .join(&format!("{}/", contest_id))?
        .join("tasks")?;
    let response = client
        .get(contest_url)
        .headers(cookies.clone().unwrap_or_default())
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
        env::current_dir()?
    }
    .join(contest_id);
    if root_path.exists() {
        return Err(Error::Invalid(format!("{} is already exists", contest_id)));
    }
    fs::create_dir(root_path.clone())?;
    let dependencies = if let Some(dependencies) = args.value_of("dependencies") {
        let mut reader = BufReader::new(File::open(dependencies)?);
        let mut buf = String::new();
        reader.read_to_string(&mut buf)?;
        buf
    } else {
        r#"proconio = { version = "=0.3.6", features = ["derive"] }"#.to_owned()
    };
    OpenOptions::new()
        .write(true)
        .create(true)
        .open(root_path.join("Cargo.toml"))?
        .write_all(
            generator::generate_cargo_toml(contest_id, username, &dependencies).as_bytes(),
        )?;
    let src_path = root_path.join("src");
    let sample_keys: Vec<_> = samples.keys().map(|key| key.to_lowercase()).collect();
    fs::create_dir(src_path.clone())?;
    OpenOptions::new()
        .write(true)
        .create(true)
        .open(src_path.join("main.rs"))?
        .write_all(generator::generate_main_rs(sample_keys).as_bytes())?;
    let template = if let Some(template) = args.value_of("template") {
        let mut reader = BufReader::new(File::open(template)?);
        let mut buf = String::new();
        reader.read_to_string(&mut buf)?;
        buf
    } else {
        r#"use proconio::input;

pub fn main() {
}
"#
        .to_owned()
    };
    samples
        .iter()
        .map(|(key, samples)| {
            OpenOptions::new()
                .write(true)
                .create(true)
                .open(src_path.join(key.to_lowercase() + ".rs"))
                .and_then(|mut options| {
                    options.write_all(
                        format!(
                            "{}\n{}",
                            template,
                            generator::generate_test_cases(&key.to_lowercase(), samples)
                        )
                        .as_bytes(),
                    )
                })
        })
        .collect::<Result<_, _>>()?;

    Ok(())
}
