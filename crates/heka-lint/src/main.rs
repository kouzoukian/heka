use std::fs;
use std::process::ExitCode;

use heka_lint::{BASELINE, Error, baseline, judge, report, repository, scan, tighten};

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("heka-lint: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: Vec<String>) -> Result<bool, Error> {
    let root = repository();
    let found = scan(&root)?;
    let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
    let update = match arguments[..] {
        [] => false,
        ["--update"] => true,
        ["--list"] => {
            for comment in &found {
                println!("{}:{}: {}", comment.path, comment.line, comment.text);
            }
            return Ok(true);
        }
        _ => return Err("usage: heka-lint [--update | --list]".into()),
    };
    let allowed = baseline(&root)?;
    let mut verdict = judge(&found, &allowed);
    if update && !verdict.gone.is_empty() {
        fs::write(root.join(BASELINE), tighten(&allowed, &verdict.gone))?;
        println!(
            "removed {} line(s) from {BASELINE}",
            std::mem::take(&mut verdict.gone).len()
        );
    }
    if !verdict.passes() {
        eprintln!("{}", report(&verdict));
    }
    Ok(verdict.passes())
}
