use std::cell::RefCell;
use std::time::{Duration, Instant};

use heka_core::release::{CheckError, ReleaseCheck, Version};
use heka_core::web::{Answer, Request, Web};

struct Fake {
    answer: Result<Answer, String>,
    asked: RefCell<Vec<Request>>,
}

impl Fake {
    fn answering(status: u16, body: &str) -> Self {
        Self {
            answer: Ok(Answer {
                status,
                body: body.as_bytes().to_vec(),
            }),
            asked: RefCell::default(),
        }
    }

    fn latest(tag: &str) -> Self {
        Self::answering(
            200,
            &format!(r#"{{"tag_name": "{tag}", "draft": false, "prerelease": false}}"#),
        )
    }
}

impl Web for Fake {
    fn get(&self, request: &Request) -> Result<Answer, String> {
        self.asked.borrow_mut().push(request.clone());
        self.answer.clone()
    }
}

fn version(text: &str) -> Version {
    Version::parse(text).expect("a version")
}

fn check(running: &'static str) -> ReleaseCheck {
    ReleaseCheck::new(running)
}

#[test]
fn a_newer_release_is_reported_with_its_page() {
    let web = Fake::latest("v0.2.0");
    let mut check = check("0.1.1");

    let found = check.poll(&web, true, Instant::now());

    assert_eq!(found, Some(Ok(Some(version("0.2.0")))));
    assert_eq!(check.newer(), Some(version("0.2.0")));
    assert_eq!(version("0.2.0").to_string(), "0.2.0");
    assert_eq!(
        version("0.2.0").release_page(),
        "https://github.com/kouzoukian/heka/releases/tag/v0.2.0"
    );
}

fn found(web: &Fake, running: &'static str) -> Result<Option<Version>, CheckError> {
    check(running)
        .poll(web, true, Instant::now())
        .expect("the first poll checks")
}

#[test]
fn only_a_later_version_is_newer() {
    for (latest, running) in [
        ("v0.1.2", "0.1.1"),
        ("v0.2.0", "0.1.9"),
        ("v1.0.0", "0.9.9"),
        ("v0.10.0", "0.9.0"),
        ("v0.1.10", "0.1.9"),
    ] {
        let web = Fake::latest(latest);
        let newer = version(&latest[1..]);
        assert_eq!(found(&web, running), Ok(Some(newer)), "{latest} {running}");
    }
    for (latest, running) in [
        ("v0.1.1", "0.1.1"),
        ("v0.1.0", "0.1.1"),
        ("v0.9.0", "0.10.0"),
        ("v0.9.9", "1.0.0"),
    ] {
        let web = Fake::latest(latest);
        assert_eq!(found(&web, running), Ok(None), "{latest} {running}");
    }
}

#[test]
fn a_release_is_newer_than_its_own_pre_release() {
    assert_eq!(
        found(&Fake::latest("v0.2.0"), "0.2.0-rc.1"),
        Ok(Some(version("0.2.0")))
    );
    assert_eq!(found(&Fake::latest("v0.1.9"), "0.2.0-rc.1"), Ok(None));
}

#[test]
fn a_pre_release_or_a_draft_is_never_newer() {
    for body in [
        r#"{"tag_name": "v0.2.0", "prerelease": true}"#,
        r#"{"tag_name": "v0.2.0", "draft": true}"#,
        r#"{"tag_name": "v0.2.0-rc.1", "prerelease": false}"#,
        r#"{"tag_name": "v0.2.0-rc.1", "prerelease": true}"#,
    ] {
        let web = Fake::answering(200, body);
        assert_eq!(found(&web, "0.1.1"), Ok(None), "{body}");
    }
}

#[test]
fn the_rest_of_githubs_answer_is_ignored() {
    let body = r#"{
        "url": "https://api.github.com/repos/kouzoukian/heka/releases/1",
        "html_url": "https://example.com/elsewhere",
        "tag_name": "v0.2.0",
        "name": "heka v0.2.0",
        "draft": false,
        "prerelease": false,
        "assets": [{"name": "heka-v0.2.0-windows-x64.zip", "size": 1}],
        "body": "notes"
    }"#;

    assert_eq!(
        found(&Fake::answering(200, body), "0.1.1"),
        Ok(Some(version("0.2.0")))
    );
}

#[test]
fn only_three_plain_numbers_are_a_version() {
    assert!(Version::parse("0.0.0").is_some());
    for text in [
        "",
        "1",
        "1.2",
        "1.2.3.4",
        "1.2.x",
        "1..3",
        "v1.2.3",
        "+1.2.3",
        "01.2.3",
        "1.2.3 ",
        "1.2.3-rc.1",
        "99999999999999999999.0.0",
    ] {
        assert_eq!(Version::parse(text), None, "{text:?}");
    }
}

#[test]
fn the_version_heka_is_built_as_can_be_compared() {
    let running = env!("CARGO_PKG_VERSION");
    let web = Fake::latest("v999.0.0");

    assert_eq!(found(&web, running), Ok(Some(version("999.0.0"))));
}

#[test]
fn github_is_asked_with_nothing_but_heka_and_its_version() {
    let web = Fake::latest("v0.2.0");

    let _ = found(&web, "0.1.1");

    assert_eq!(
        *web.asked.borrow(),
        [Request {
            host: "api.github.com",
            path: "/repos/kouzoukian/heka/releases/latest".into(),
            user_agent: "heka/0.1.1".into(),
            accept: "application/vnd.github+json",
        }]
    );
}

#[test]
fn a_failed_check_says_why_and_reports_no_release() {
    let offline = Fake {
        answer: Err("no network".into()),
        asked: RefCell::default(),
    };
    let cases = [
        (offline, CheckError::NotFetched("no network".into())),
        (Fake::answering(404, "{}"), CheckError::Refused(404)),
        (
            Fake::answering(403, r#"{"tag_name": "v9.0.0"}"#),
            CheckError::Refused(403),
        ),
        (
            Fake::latest("nightly"),
            CheckError::UnknownTag("nightly".into()),
        ),
        (
            Fake::latest("0.2.0"),
            CheckError::UnknownTag("0.2.0".into()),
        ),
    ];
    for (web, why) in cases {
        let mut check = check("0.1.1");
        assert_eq!(
            check.poll(&web, true, Instant::now()),
            Some(Err(why.clone())),
            "{why:?}"
        );
        assert_eq!(check.newer(), None);
    }
}

#[test]
fn an_answer_that_is_no_release_is_a_failure() {
    for body in ["", "<html>", "[]", "{}", r#"{"tag_name": 2}"#] {
        let web = Fake::answering(200, body);
        assert!(
            matches!(found(&web, "0.1.1"), Err(CheckError::Unreadable(_))),
            "{body:?}"
        );
    }
}

fn after(start: Instant, seconds: u64) -> Instant {
    start + Duration::from_secs(seconds)
}

const HOUR: u64 = 60 * 60;
const DAY: u64 = 24 * HOUR;

#[test]
fn heka_checks_at_start_and_once_a_day() {
    let web = Fake::latest("v0.1.1");
    let mut check = check("0.1.1");
    let start = Instant::now();

    assert!(check.poll(&web, true, start).is_some());
    assert_eq!(check.poll(&web, true, after(start, 1)), None);
    assert_eq!(check.poll(&web, true, after(start, DAY - 1)), None);
    assert!(check.poll(&web, true, after(start, DAY)).is_some());
    assert_eq!(check.poll(&web, true, after(start, DAY + 60)), None);
    assert!(check.poll(&web, true, after(start, 2 * DAY)).is_some());
    assert_eq!(web.asked.borrow().len(), 3);
}

#[test]
fn a_failed_check_is_made_again_every_hour_until_one_succeeds() {
    let offline = Fake {
        answer: Err("no network".into()),
        asked: RefCell::default(),
    };
    let refusing = Fake::answering(500, "");
    let online = Fake::latest("v0.2.0");
    let mut check = check("0.1.1");
    let start = Instant::now();

    assert!(check.poll(&offline, true, start).is_some());
    assert_eq!(check.poll(&offline, true, after(start, HOUR - 1)), None);
    assert!(check.poll(&refusing, true, after(start, HOUR)).is_some());
    assert_eq!(
        check.poll(&refusing, true, after(start, 2 * HOUR - 1)),
        None
    );
    assert!(check.poll(&online, true, after(start, 2 * HOUR)).is_some());
    assert_eq!(check.newer(), Some(version("0.2.0")));

    assert_eq!(check.poll(&online, true, after(start, 3 * HOUR)), None);
    assert_eq!(
        check.poll(&online, true, after(start, 2 * HOUR + DAY - 1)),
        None
    );
    assert!(
        check
            .poll(&online, true, after(start, 2 * HOUR + DAY))
            .is_some()
    );
}

#[test]
fn switched_off_and_on_a_failed_check_still_waits_its_hour() {
    let web = Fake::answering(500, "");
    let mut check = check("0.1.1");
    let start = Instant::now();

    assert!(check.poll(&web, true, start).is_some());
    assert_eq!(check.poll(&web, false, after(start, 10)), None);
    assert_eq!(check.poll(&web, true, after(start, 20)), None);
    assert_eq!(check.poll(&web, false, after(start, HOUR)), None);
    assert!(check.poll(&web, true, after(start, HOUR + 1)).is_some());
}

#[test]
fn switched_off_heka_asks_nothing() {
    let web = Fake::latest("v0.2.0");
    let mut check = check("0.1.1");
    let start = Instant::now();

    assert_eq!(check.poll(&web, false, start), None);
    assert_eq!(check.poll(&web, false, after(start, 3 * DAY)), None);

    assert!(web.asked.borrow().is_empty());
    assert_eq!(check.newer(), None);
}

#[test]
fn switched_on_heka_checks_at_once_unless_it_did_within_the_day() {
    let web = Fake::latest("v0.2.0");
    let mut check = check("0.1.1");
    let start = Instant::now();

    assert_eq!(check.poll(&web, false, start), None);
    assert!(check.poll(&web, true, after(start, 5)).is_some());
    assert_eq!(check.poll(&web, false, after(start, 10)), None);
    assert_eq!(check.poll(&web, true, after(start, 15)), None);
    assert_eq!(web.asked.borrow().len(), 1);
}

#[test]
fn a_known_release_outlasts_a_failed_check_but_not_a_withdrawn_one() {
    let mut check = check("0.1.1");
    let start = Instant::now();

    check.poll(&Fake::latest("v0.2.0"), true, start);
    check.poll(&Fake::answering(500, ""), true, after(start, DAY));
    assert_eq!(check.newer(), Some(version("0.2.0")));

    check.poll(&Fake::latest("v0.1.1"), true, after(start, 2 * DAY));
    assert_eq!(check.newer(), None);
}
