pub mod lexer;

use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

use lexer::Language;

pub const BASELINE: &str = "comment-baseline.txt";

pub type Error = Box<dyn std::error::Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    pub path: String,
    pub line: usize,
    pub text: String,
}

impl Comment {
    pub fn entry(&self) -> String {
        let text = self
            .text
            .replace('\\', r"\\")
            .replace('\r', "")
            .replace('\n', r"\n");
        format!("{}\t{text}", self.path)
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Verdict {
    pub added: Vec<Comment>,
    pub gone: Vec<String>,
}

impl Verdict {
    pub fn passes(&self) -> bool {
        self.added.is_empty() && self.gone.is_empty()
    }
}

pub fn comments_in(path: &str, source: &str) -> Vec<Comment> {
    let Some(language) = Language::of(path) else {
        return Vec::new();
    };
    language
        .comments(source)
        .into_iter()
        .map(|range| Comment {
            path: path.to_owned(),
            line: source[..range.start].matches('\n').count() + 1,
            text: source[range].to_owned(),
        })
        .collect()
}

pub fn judge(found: &[Comment], baseline: &str) -> Verdict {
    let mut allowed: HashMap<&str, usize> = HashMap::new();
    for entry in baseline.lines() {
        *allowed.entry(entry).or_default() += 1;
    }
    let mut added = Vec::new();
    for comment in found {
        match allowed.get_mut(comment.entry().as_str()) {
            Some(left) if *left > 0 => *left -= 1,
            _ => added.push(comment.clone()),
        }
    }
    let mut gone = Vec::new();
    for entry in baseline.lines() {
        if let Some(left) = allowed.get_mut(entry)
            && *left > 0
        {
            *left -= 1;
            gone.push(entry.to_owned());
        }
    }
    Verdict { added, gone }
}

pub fn tighten(baseline: &str, gone: &[String]) -> String {
    let mut gone: Vec<&String> = gone.iter().collect();
    let mut kept = String::new();
    for entry in baseline.lines() {
        match gone.iter().position(|removed| *removed == entry) {
            Some(index) => {
                gone.swap_remove(index);
            }
            None => {
                kept.push_str(entry);
                kept.push('\n');
            }
        }
    }
    kept
}

pub fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn scan(root: &Path) -> Result<Vec<Comment>, Error> {
    let listed = Command::new("git")
        .current_dir(root)
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .output()?;
    if !listed.status.success() {
        return Err(String::from_utf8_lossy(&listed.stderr).into_owned().into());
    }
    let mut found = Vec::new();
    for path in String::from_utf8(listed.stdout)?.split_terminator('\0') {
        if Language::of(path).is_none() {
            continue;
        }
        match fs::read_to_string(root.join(path)) {
            Ok(source) => found.extend(comments_in(path, &source)),
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(format!("{path}: {error}").into()),
        }
    }
    Ok(found)
}

pub fn baseline(root: &Path) -> Result<String, Error> {
    match fs::read_to_string(root.join(BASELINE)) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error.into()),
    }
}

pub fn report(verdict: &Verdict) -> String {
    let mut lines = Vec::new();
    for comment in &verdict.added {
        let first = comment.text.lines().next().unwrap_or_default();
        lines.push(format!("{}:{}: {first}", comment.path, comment.line));
    }
    if !verdict.added.is_empty() {
        lines.push(format!(
            "{} comment(s) not allowed (docs/adr/0003-no-code-comments.md). Run /delete-comment.",
            verdict.added.len()
        ));
    }
    if !verdict.gone.is_empty() {
        lines.push(format!(
            "{} comment(s) in {BASELINE} are gone. Run `cargo run -p heka-lint -- --update`.",
            verdict.gone.len()
        ));
    }
    lines.join("\n")
}
