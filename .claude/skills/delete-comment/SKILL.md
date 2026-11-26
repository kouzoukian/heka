---
name: delete-comment
description: Delete code comments, moving what they hold into ADRs and GitHub issues.
argument-hint: "[path]"
disable-model-invocation: true
---

# Delete comments

Why the repository bans comments: `docs/adr/0003-no-code-comments.md`.

## 1. List

```
cargo run -q -p heka-lint -- --list
```

This prints every comment, listed in `comment-baseline.txt` or new. With a path argument, work only on comments under it. No output means there is nothing to do: say so and stop.

## 2. Categorise

Read each comment together with the code it describes, then give it exactly one category. Check its claims against the code before believing them.

| Category            | Test                                                                        | Action                         |
| ------------------- | --------------------------------------------------------------------------- | ------------------------------ |
| **Tautology**       | The names, types or adjacent code already say it                            | Delete                         |
| **Stale**           | It is wrong now, or points at code or facts that are gone                   | Delete                         |
| **Excuse**          | It makes a shortcut acceptable: a swallowed error, magic number, duplicate, sleep, workaround or known gap | Delete, and file an issue |
| **Decision record** | A non-obvious why, constraint, external source or invariant                 | Move into an ADR, then delete  |

- A comment that mixes categories takes the strongest: Excuse, then Decision record, then Stale, then Tautology. Carry only its true parts forward.
- A Stale comment that was describing what the code *should* do points at a bug: treat it as an Excuse.
- A `SAFETY:` comment is a Decision record.

## 3. Excuses

One issue per shortcut, covering every comment that excused it. Search open issues first (`gh issue list --state open --limit 200`) and reuse one that already covers it. Otherwise file it as `docs/agents/issue-tracker.md` describes, labelled `needs-triage` plus `bug` or `enhancement`, with `## Problem` and `## Acceptance criteria` sections. Name files and symbols, not line numbers, and write it so that someone who never saw the comment understands it. Leave the code as it is.

## 4. Decision records

Add to the ADR in `docs/adr/` that already covers the area; write a new one only for a new area, numbered next. Use the terms in `CONTEXT.md`. State the decision and its reason, keeping every external source (file, symbol, version, commit) and every number the code cannot explain. Where an ADR and the code disagree, the code is the fact: say so to the user.

## 5. Delete

Delete a comment only once its issue or ADR text exists. Remove the whole line when nothing else is on it, then run:

```
cargo fmt --all
```

```
cargo run -q -p heka-lint -- --update
```

`--update` shrinks `comment-baseline.txt`. Comments you did not get to stay listed there; a comment the lint reports as not allowed is one to delete, never one to add to the baseline.

## 6. Verify

Done when every comment in scope has a category and its action, and these pass:

```
cargo clippy --workspace --all-targets --locked -- -D warnings
```

```
cargo test --workspace --locked
```

Report the count per category, the issues filed or reused, the ADRs touched, and what is left in the baseline.
