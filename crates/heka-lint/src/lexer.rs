use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    Toml,
    Yaml,
    GitIgnore,
    Hex,
}

impl Language {
    pub fn of(path: &str) -> Option<Self> {
        let name = path.rsplit('/').next().unwrap_or(path);
        if name == ".gitignore" {
            return Some(Self::GitIgnore);
        }
        match name.rsplit_once('.')?.1 {
            "rs" => Some(Self::Rust),
            "toml" => Some(Self::Toml),
            "yml" | "yaml" => Some(Self::Yaml),
            "hex" => Some(Self::Hex),
            _ => None,
        }
    }

    pub fn comments(self, source: &str) -> Vec<Range<usize>> {
        match self {
            Self::Rust => rust(source),
            Self::Toml => toml(source),
            Self::Yaml => by_line(source, yaml_comment),
            Self::GitIgnore => by_line(source, |line| line.starts_with('#').then_some(0)),
            Self::Hex => by_line(source, |line| line.find('#')),
        }
    }
}

fn by_line(source: &str, comment_at: impl Fn(&str) -> Option<usize>) -> Vec<Range<usize>> {
    let mut start = 0;
    let mut found = Vec::new();
    for line in source.split_inclusive('\n') {
        let text = line.trim_end_matches(['\r', '\n']);
        if let Some(at) = comment_at(text) {
            found.push(start + at..start + text.len());
        }
        start += line.len();
    }
    found
}

fn yaml_comment(line: &str) -> Option<usize> {
    let mut quote = None;
    let mut previous = None;
    let mut characters = line.char_indices();
    while let Some((at, character)) = characters.next() {
        let after_break = previous.is_none_or(|p: char| p.is_whitespace());
        let opens_value = after_break || previous.is_some_and(|p| "[{,:=(".contains(p));
        match quote {
            Some('"') if character == '\\' => {
                characters.next();
            }
            Some(open) if character == open => quote = None,
            Some(_) => {}
            None if matches!(character, '"' | '\'') && opens_value => quote = Some(character),
            None if character == '#' && after_break => return Some(at),
            None => {}
        }
        previous = Some(character);
    }
    None
}

fn toml(source: &str) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(character) = source[at..].chars().next() {
        let rest = &source[at..];
        at += match character {
            '#' => {
                let length = line_length(rest);
                found.push(at..at + length);
                length
            }
            '"' if rest.starts_with(r#"""""#) => quoted(rest, r#"""""#, true),
            '\'' if rest.starts_with("'''") => quoted(rest, "'''", false),
            '"' => quoted(rest, "\"", true),
            '\'' => quoted(rest, "'", false),
            other => other.len_utf8(),
        };
    }
    found
}

fn rust(source: &str) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(character) = source[at..].chars().next() {
        let rest = &source[at..];
        at += match character {
            '/' if rest.starts_with("//") => {
                let length = line_length(rest);
                found.push(at..at + length);
                length
            }
            '/' if rest.starts_with("/*") => {
                let length = block_length(rest);
                found.push(at..at + length);
                length
            }
            '"' => quoted(rest, "\"", true),
            '\'' => character_or_lifetime(rest),
            c if c.is_alphanumeric() || c == '_' => word_or_raw_string(rest),
            other => other.len_utf8(),
        };
    }
    found
}

fn line_length(rest: &str) -> usize {
    let line = rest.split('\n').next().unwrap_or(rest);
    line.trim_end_matches('\r').len()
}

fn block_length(rest: &str) -> usize {
    let mut depth = 0;
    let mut at = 0;
    while at < rest.len() {
        if rest.as_bytes()[at..].starts_with(b"/*") {
            depth += 1;
            at += 2;
        } else if rest.as_bytes()[at..].starts_with(b"*/") {
            depth -= 1;
            at += 2;
            if depth == 0 {
                return at;
            }
        } else {
            at += 1;
        }
    }
    rest.len()
}

fn quoted(rest: &str, delimiter: &str, escapes: bool) -> usize {
    let mut at = delimiter.len();
    while at < rest.len() {
        if rest.as_bytes()[at..].starts_with(delimiter.as_bytes()) {
            return at + delimiter.len();
        }
        at += if escapes && rest.as_bytes()[at] == b'\\' {
            2
        } else {
            1
        };
        while !rest.is_char_boundary(at.min(rest.len())) {
            at += 1;
        }
    }
    rest.len()
}

fn character_or_lifetime(rest: &str) -> usize {
    let mut characters = rest.char_indices().skip(1);
    match (characters.next(), characters.next()) {
        (Some((_, '\\')), _) => quoted(rest, "'", true),
        (Some(_), Some((close, '\''))) => close + 1,
        _ => 1,
    }
}

fn word_or_raw_string(rest: &str) -> usize {
    let length = rest
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(rest.len());
    let (word, after) = rest.split_at(length);
    if !matches!(word, "r" | "br" | "cr") {
        return length;
    }
    let hashes = after.len() - after.trim_start_matches('#').len();
    let Some(body) = after[hashes..].strip_prefix('"') else {
        return length;
    };
    let close = format!("\"{}", "#".repeat(hashes));
    body.find(&close)
        .map_or(rest.len(), |end| length + hashes + 1 + end + close.len())
}
