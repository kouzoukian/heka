use std::fmt;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::web::{Request, Web};

const GITHUB: &str = "https://github.com/";
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
const API_HOST: &str = "api.github.com";
const ACCEPT: &str = "application/vnd.github+json";
const OK: u16 = 200;

pub const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    major: u64,
    minor: u64,
    patch: u64,
}

impl Version {
    pub fn parse(text: &str) -> Option<Self> {
        let mut numbers = text.split('.').map(number);
        let version = Self {
            major: numbers.next()??,
            minor: numbers.next()??,
            patch: numbers.next()??,
        };
        numbers.next().is_none().then_some(version)
    }

    pub fn release_page(self) -> String {
        format!("{REPOSITORY}/releases/tag/v{self}")
    }
}

fn number(text: &str) -> Option<u64> {
    let digits = text.bytes().all(|byte| byte.is_ascii_digit());
    let padded = text.len() > 1 && text.starts_with('0');
    (digits && !padded).then(|| text.parse().ok())?
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            major,
            minor,
            patch,
        } = self;
        write!(f, "{major}.{minor}.{patch}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckError {
    NotFetched(String),
    Refused(u16),
    Unreadable(String),
    UnknownTag(String),
}

pub struct ReleaseCheck {
    running: &'static str,
    checked: Option<Instant>,
    newer: Option<Version>,
}

impl ReleaseCheck {
    pub fn new(running: &'static str) -> Self {
        Self {
            running,
            checked: None,
            newer: None,
        }
    }

    pub fn poll(
        &mut self,
        web: &dyn Web,
        enabled: bool,
        now: Instant,
    ) -> Option<Result<Option<Version>, CheckError>> {
        let recently = self
            .checked
            .is_some_and(|at| now.saturating_duration_since(at) < CHECK_EVERY);
        if !enabled || recently {
            return None;
        }
        self.checked = Some(now);
        let found = self.ask(web);
        if let Ok(newer) = found {
            self.newer = newer;
        }
        Some(found)
    }

    pub fn newer(&self) -> Option<Version> {
        self.newer
    }

    fn ask(&self, web: &dyn Web) -> Result<Option<Version>, CheckError> {
        let answer = web
            .get(&request(self.running))
            .map_err(CheckError::NotFetched)?;
        if answer.status != OK {
            return Err(CheckError::Refused(answer.status));
        }
        let latest = latest(&answer.body)?;
        Ok(latest.filter(|&latest| newer_than(latest, self.running)))
    }
}

fn request(running: &str) -> Request {
    let repository = REPOSITORY.trim_start_matches(GITHUB);
    Request {
        host: API_HOST,
        path: format!("/repos/{repository}/releases/latest"),
        user_agent: format!("heka/{running}"),
        accept: ACCEPT,
    }
}

#[derive(Deserialize)]
struct Latest {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

fn latest(body: &[u8]) -> Result<Option<Version>, CheckError> {
    let latest: Latest =
        serde_json::from_slice(body).map_err(|error| CheckError::Unreadable(error.to_string()))?;
    if latest.draft || latest.prerelease {
        return Ok(None);
    }
    let Some(tagged) = latest.tag_name.strip_prefix('v') else {
        return Err(CheckError::UnknownTag(latest.tag_name));
    };
    let (numbers, pre_release) = split_pre_release(tagged);
    match Version::parse(numbers) {
        Some(version) => Ok((!pre_release).then_some(version)),
        None => Err(CheckError::UnknownTag(latest.tag_name)),
    }
}

fn newer_than(release: Version, running: &str) -> bool {
    let (numbers, pre_release) = split_pre_release(running);
    Version::parse(numbers)
        .is_some_and(|running| release > running || (pre_release && release == running))
}

fn split_pre_release(version: &str) -> (&str, bool) {
    match version.split_once('-') {
        Some((numbers, _)) => (numbers, true),
        None => (version, false),
    }
}
