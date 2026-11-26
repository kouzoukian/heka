# No code comments

Code carries no comments, doc comments included. Names, types and tests say what the code does; a decision that the code cannot show goes into an ADR; work that is still owed goes into a GitHub issue. Comments had drifted from the code, mostly repeated it, and let a shortcut stand because a sentence next to it explained it.

`cargo run -p heka-lint` enforces this over every tracked or new `.rs`, `.toml`, `.yml`, `.yaml`, `.hex` and `.gitignore` file, and the same check runs as a test, so CI fails on a new comment. Markdown and the JSON theme are not code and are not read.

It is a ratchet. `comment-baseline.txt` lists the comments that are still allowed, one line each: the file and the comment's text. A comment that is not listed fails the check, so one cannot be added, edited or swapped for another. A listed comment that is gone also fails it, until `cargo run -p heka-lint -- --update` removes the line. `--update` never adds a line. The `/delete-comment` skill works through whatever is listed or newly written.

`heka-lint` reads files itself and has no dependencies: neither Clippy nor rustfmt can ban comments, and a Dylint library would need a nightly toolchain.

`SAFETY:` comments are not excepted. What makes each `unsafe` block sound is recorded in the ADR for its area, and Clippy's `undocumented_unsafe_blocks` stays off.
