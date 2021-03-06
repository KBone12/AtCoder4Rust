/// Generate Cargo.toml as a String
pub fn generate_cargo_toml(project_name: &str, author: Option<&str>, dependencies: &str) -> String {
    format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
authors = ["{author}"]
edition = "2018"

[[bin]]
name = "{name}"
path = "src/main.rs"

[dependencies]
{dependencies}
"#,
        name = project_name,
        author = author.unwrap_or_default(),
        dependencies = dependencies
    )
}

/// Generate main.rs as a String
pub fn generate_main_rs(task_names: Vec<String>) -> String {
    let mut task_names = task_names;
    task_names.sort();
    let mods: String = task_names
        .iter()
        .map(|task| format!("mod {};\n", task))
        .collect();
    let matches = task_names
        .iter()
        .map(|task| format!(r#"        "{task}" => {task}::main(),"#, task = task))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"{mods}
fn main() {{
    let mut args = std::env::args();
    if args.len() < 2 {{
        return;
    }}
    match args.nth(1).unwrap().as_str() {{
{matches}
        _ => {{}},
    }}
}}
"#,
        mods = mods,
        matches = matches
    )
}

/// Generate a test as a String which check that the function passes this sample case
pub fn generate_sample(
    project_name: &str,
    module_name: &str,
    sample_name: &str,
    input: &str,
    output: &str,
) -> String {
    format!(
        r##"    #[test]
    fn {sample_name}() {{
        let test_dir = TestDir::new("./{project_name}", "");
        let output = test_dir
            .cmd()
            .arg("{module_name}")
            .output_with_stdin(r#"{input}"#)
            .expect_success();
        let stderr = output.stderr_str();
        if !stderr.is_empty() {{
            eprintln!("=== stderr ===");
            eprint!("{{}}", stderr);
            eprintln!("==============");
        }}
        assert_eq!(output.stdout_str(), r#"{output}"#);
        assert!(stderr.is_empty(), "stderr is not empty");
    }}
"##,
        project_name = project_name,
        sample_name = sample_name,
        module_name = module_name,
        input = input,
        output = output
    )
}

/// Generate a `tests` module as a String which check that the funciton passes all sample cases
pub fn generate_test_cases(
    project_name: &str,
    module_name: &str,
    samples: &[(String, String)],
) -> String {
    let samples: String = samples
        .iter()
        .enumerate()
        .map(|(index, (input, output))| {
            generate_sample(
                project_name,
                module_name,
                &format!("sample_{}", index + 1),
                input,
                output,
            )
        })
        .collect();
    format!(
        r#"#[cfg(test)]
mod tests {{
    use cli_test_dir::*;

{samples}
}}
"#,
        samples = samples
    )
}
