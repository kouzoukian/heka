use heka_lint::{comments_in, judge, tighten};

fn texts(path: &str, source: &str) -> Vec<String> {
    comments_in(path, source)
        .into_iter()
        .map(|comment| comment.text)
        .collect()
}

#[test]
fn every_kind_of_rust_comment_is_found() {
    let source = "//! a\r\n/// b\r\nfn f() {} // c\r\n/* d /* e */\n f */ struct S;\n/** g */\n";
    assert_eq!(
        texts("a.rs", source),
        ["//! a", "/// b", "// c", "/* d /* e */\n f */", "/** g */",]
    );
}

#[test]
fn comment_marks_inside_rust_literals_are_not_comments() {
    let source = r####"
        const A: &str = "https://example.com /* no */";
        const B: &str = "quote \" // no";
        const C: &str = r"C:\ // no \";
        const D: &str = r##"one "# // no"##;
        const E: &[u8] = br#"// no"#;
        const F: [char; 5] = ['"', '\'', '\\', '/', 'é'];
        const G: u8 = b'"';
        fn f<'a>(x: &'a str) -> &'a str { x }
        let r#type = 1;
    "####;
    assert_eq!(texts("a.rs", source), [""; 0]);
}

#[test]
fn a_comment_after_literals_is_still_found() {
    let source = "f('\"', b'\\'', r#\"\"#, \"é\\\\\"); // yes\nfn g<'a>() {} // too\n";
    assert_eq!(texts("a.rs", source), ["// yes", "// too"]);
}

#[test]
fn toml_comments_are_found_outside_strings_only() {
    let source =
        "# a\nx = \"# no\" # b\ny = '# no'\nz = \"\"\"\n# no\n\"\"\"\nw = '''\n# no'''#c\n";
    assert_eq!(texts("Cargo.toml", source), ["# a", "# b", "#c"]);
}

#[test]
fn yaml_comments_are_found_outside_quotes_only() {
    let source = "# a\nname: C# build # b\nx: \"# no\"\ny: ['# no']\nrun: |\n  # c\n  echo \"it's # no\"\nz: it's # d\n";
    assert_eq!(
        texts(".github/workflows/ci.yml", source),
        ["# a", "# b", "# c", "# d"]
    );
}

#[test]
fn gitignore_and_fixture_comments_are_found() {
    assert_eq!(texts(".gitignore", "# a\n/target\n\\#b\nc#\n"), ["# a"]);
    assert_eq!(texts("x/a.hex", "# a\n01 02 # b\n"), ["# a", "# b"]);
}

#[test]
fn other_files_are_not_read() {
    assert_eq!(texts("README.md", "<!-- a --> // b # c"), [""; 0]);
    assert_eq!(texts("theme.json", "// a"), [""; 0]);
}

#[test]
fn lines_are_numbered_from_one() {
    let found = comments_in("a.rs", "fn f() {}\n\n// here\n");
    assert_eq!(found[0].line, 3);
}

#[test]
fn a_comment_the_baseline_does_not_list_fails() {
    let verdict = judge(&comments_in("a.rs", "// new\n"), "");
    assert_eq!(verdict.added.len(), 1);
    assert!(!verdict.passes());
}

#[test]
fn a_listed_comment_passes_wherever_it_moves_in_its_file() {
    let found = comments_in("a.rs", "\n\n// old\n");
    assert!(judge(&found, "a.rs\t// old\n").passes());
}

#[test]
fn a_listed_comment_cannot_be_edited_copied_or_moved_to_another_file() {
    let baseline = "a.rs\t// old\n";
    for (path, source) in [
        ("a.rs", "// older\n"),
        ("a.rs", "// old\n// old\n"),
        ("b.rs", "// old\n"),
    ] {
        let verdict = judge(&comments_in(path, source), baseline);
        assert_eq!(verdict.added.len(), 1, "{path} {source}");
    }
}

#[test]
fn a_deleted_comment_must_leave_the_baseline() {
    let baseline = "a.rs\t// one\na.rs\t// two\na.rs\t// two\n";
    let verdict = judge(&comments_in("a.rs", "// two\n"), baseline);
    assert_eq!(verdict.gone, ["a.rs\t// one", "a.rs\t// two"]);
    assert!(!verdict.passes());
    assert_eq!(tighten(baseline, &verdict.gone), "a.rs\t// two\n");
}

#[test]
fn a_block_comment_takes_one_baseline_line() {
    let found = comments_in("a.rs", "/* a\\n\r\n b */\n");
    assert_eq!(found[0].entry(), "a.rs\t/* a\\\\n\\n b */");
}
