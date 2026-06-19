# CLAUDE.md

## Commands

- Build: `cargo build`
- Test: `cargo test`
- Test (single): `cargo test <test_name>`
- Lint: `cargo clippy --all-targets --all-features -- -D warnings`
- Format: `cargo fmt --all`
- Format check: `cargo fmt --all -- --check`
- Run: `cargo run -- <args>`

## Workflow rules

- Always run `cargo fmt --all` before committing
- Run `cargo clippy --all-targets --all-features -- -D warnings` — warnings are errors in CI
- Run `cargo test` to verify nothing is broken
- MSRV is **1.75** — do not use Rust features unavailable at this version
- Binary name is `slack` (defined in Cargo.toml `[[bin]]`)
- JSON output is the default; `--plain` gives TSV — both paths must be maintained

## Project guardrails

- Do not break CLI interface (commands, flags, exit codes) without updating README.md and tests
- Do not commit tokens or secrets — auth uses system keyring or `SLACK_TOKEN_STORE_PATH`
- Do not add unnecessary dependencies — this is a single-binary CLI tool
- Integration tests require `SLACK_INTEGRATION_TESTS=1` — unit tests run without it

## Where to look

- `README.md` — Full CLI usage, all commands, env vars, troubleshooting
- `src/cli/` — One file per command group (auth, channels, messages, etc.)
- `src/api/` — Slack API client, rate limiter, channel/user resolution
- `src/auth/` — Token types, keyring storage, OAuth flow, browser token support
- `src/models/` — Shared data types (channel, message, user, file, reaction)
- `tests/integration/` — Integration tests (need real or mocked Slack API)
- `.github/workflows/ci.yml` — CI checks (test, fmt, clippy, docs, coverage)
