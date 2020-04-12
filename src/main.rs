use clap::{app_from_crate, crate_authors, crate_description, crate_name, crate_version, Arg};

fn main() {
    let args = app_from_crate!()
        .arg(Arg::with_name("contest id").index(1).required(true))
        .get_matches();
    let contest_id = args.value_of("contest id").unwrap();
    println!("contest id: {}", contest_id);
}
