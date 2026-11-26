use heka_lint::{baseline, judge, report, repository, scan};

#[test]
fn the_repository_has_no_comment_outside_the_baseline() {
    let root = repository();
    let found = scan(&root).unwrap();
    let verdict = judge(&found, &baseline(&root).unwrap());
    assert!(verdict.passes(), "\n{}", report(&verdict));
}
