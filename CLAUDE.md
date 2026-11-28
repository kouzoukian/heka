## No code comments

Write code without comments, doc comments included; `cargo run -p heka-lint` fails on one, and `pnpm --dir site check` does for the website. A non-obvious decision goes into `docs/adr/`, owed work into a GitHub issue. See `docs/adr/0003-no-code-comments.md`.

## User documentation

The user manual is `site/content/docs`; change it together with the behaviour it describes. `README.md` covers only install and building. See `docs/adr/0025-the-website.md`.

## Agent skills

### Issue tracker

Issues and specs live as GitHub issues in `kouzoukian/heka`, managed with the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

The five canonical triage roles use their default label strings. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `CONTEXT.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.
