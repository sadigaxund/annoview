# AGENTS.md

Operational rules for anyone, human or agent, who works in this repository.
`SPEC.md` lists what the app must do. Tick an item there only after it was verified.

## Task protocol

1. Read `SPEC.md` and the code the task touches.
2. State the plan and the checks that will prove it, before writing code.
3. Write the tests for the contract first when the behavior is pure logic.
4. Implement in small diffs. One topic per commit.
5. Run the real binary on real input and show the output.
6. Report what was verified, what was not, and what the owner must run.

An issue inside the task's scope is fixed in the task.

## Verification

- A green suite does not mean the feature works. Run the app on a real image.
- A guard or test is done only after it has failed on a planted defect.
- Do not parse human-readable test output. Use exit codes.
- A worker's report is a claim. Rerun the gates and read the diff before trusting it.
- "I could not verify X" is a valid result. A false "works" costs more.
- No absolute machine path enters a committed file.

## Environment

- Use `nix develop` for the toolchain. `nix build` must pass before a push.
- The machine has limited RAM. Run one build or test process at a time.
- Never run `sudo`. Print the command for the owner.

## Git and attribution

- Commit subjects are plain and short. One logical change per commit, SSH-signed.
- Never force-push or rewrite pushed history.
- No AI reference of any kind enters the repository: no session links, no `Co-Authored-By`, no tool trailers.
- Tags and releases are made by the owner after a smoke test.

## Writing rules

These apply to every committed sentence: docs, comments and commit messages.

- One fact per sentence, about 20 words, plain verbs.
- No em dashes and no en dashes.
- No "not X but Y" constructions.
- No bold mini-headings inside bullets.
- No dates or status words ("planned", "done") outside `CHANGELOG.md` and `SPEC.md`.
- Give the number or say nothing. Name the source.
- READMEs say how to install and use, in the fewest steps. No folder trees: they go stale.

## Changelog and releases

- Every user-visible change adds a line under `Unreleased` in `CHANGELOG.md` in the same commit.
- A release renames `Unreleased` to `YYYY.MM.DD` and starts a new empty `Unreleased`.
- The tag is `vYYYY.MM.DD`. The version in `Cargo.toml` and `flake.nix` matches the date without the zeros stripped, for example `2026.11.27`.
- The owner tags and publishes. Agents prepare the changelog entry and stop.

## Change triggers

| If you change | You must also |
|---|---|
| A behavior listed in `SPEC.md` | update that item |
| A key, menu item or setting | `README.md` keys table and `SPEC.md` |
| A dependency | `Cargo.lock` and the `cargoLock` in `flake.nix` still build |
| Pure logic in `src/logic/` | its unit tests in the same file |
