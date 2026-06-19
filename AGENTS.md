# AGENTS.md

## Quick start (copy/paste)

- Install: `cargo install --path .`
- Build: `cargo build`
- Test (fast): `cargo test`
- Test (single): `cargo test <test_name>`
- Lint: `cargo clippy --all-targets --all-features -- -D warnings`
- Format check: `cargo fmt --all -- --check`
- Format fix: `cargo fmt --all`
- Docs: `cargo doc --no-deps --document-private-items`
- Run locally: `cargo run -- <args>`

## Repo map

- `src/main.rs` — Entry point, CLI routing
- `src/lib.rs` — Library root (re-exports all modules)
- `src/api/` — Slack API client (web API, edge API, rate limiter, channel/user resolution)
- `src/auth/` — Authentication (token types, keyring/file storage, OAuth, browser tokens)
- `src/cli/` — CLI command handlers (auth, channels, messages, users, files, reactions, status, reminders, completions)
- `src/models/` — Data models (channel, message, user, file, reaction)
- `src/output/` — Output formatting (JSON default, plain/TSV)
- `src/error/` — Error types and exit codes
- `src/utils/` — Utilities (time limit parsing)
- `src/bin/test_keyring.rs` — Keyring diagnostic binary
- `tests/` — Unit tests (`cli_*.rs`, `api_resolve.rs`) and integration tests (`integration/`)

## Working agreements

- Verify changes compile: `cargo build`
- Verify formatting: `cargo fmt --all -- --check`
- Verify linting: `cargo clippy --all-targets --all-features -- -D warnings`
- Run targeted tests while iterating: `cargo test <test_name>`
- Before finishing, run the full suite: `cargo test`
- MSRV is **1.75** — do not use features requiring a newer Rust edition or version

## CI checks (all must pass)

CI runs on every push/PR to main. These are the exact checks:
1. `cargo build --verbose` (Linux, macOS, Windows × stable + 1.75)
2. `cargo test --verbose` (with `SLACK_INTEGRATION_TESTS=1`)
3. `cargo fmt --all -- --check`
4. `cargo clippy --all-targets --all-features -- -D warnings`
5. `cargo doc --no-deps --document-private-items` (with `RUSTDOCFLAGS=-D warnings`)
6. `cargo llvm-cov --all-features --workspace --fail-under 80`

## Guardrails (do not)

- Do not break the public CLI interface (command names, flags, exit codes) without updating README.md and tests
- Do not add production dependencies without justification — keep the binary lean
- Do not store secrets in code; tokens go through keyring (via `keyring` crate) or file store (`SLACK_TOKEN_STORE_PATH`)

## Testing notes

- Unit tests use `mockito` for HTTP mocking and `assert_cmd` for CLI binary testing
- Integration tests live in `tests/integration/` and are gated by `SLACK_INTEGRATION_TESTS=1`
- Running `cargo test` without that env var skips integration tests (safe for local dev)
- File-based token storage (`SLACK_TOKEN_STORE_PATH`) is preferred in tests to avoid keyring issues

## Environment variables

| Variable | Purpose |
|----------|---------|
| `SLACK_TOKEN` | Override token (skips keyring lookup) |
| `SLACK_WORKSPACE` | Default workspace team ID |
| `SLACK_TOKEN_STORE_PATH` | File-based token storage path (instead of keyring) |
| `SLACK_API_BASE_URL` | Override API base URL (for testing with mockito) |
| `SLACK_INTEGRATION_TESTS` | Set to `1` to enable integration tests |

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Authentication required |
| 3 | Invalid arguments |
| 4 | API error |
| 5 | Rate limited |
| 6 | Network error |

## Release

Releases are triggered by pushing a `v*` tag. The GitHub Actions release workflow cross-compiles for Linux (x86_64, aarch64, musl), macOS (x86_64, aarch64), and Windows (x86_64, aarch64), then creates a draft GitHub release.
