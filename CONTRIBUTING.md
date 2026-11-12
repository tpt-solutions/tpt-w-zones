# Contributing to tpt-zones

Thanks for your interest in tpt-zones. This document covers how to set up, what we
expect from changes, and how to get them merged.

## Getting started

1. Fork the repository and clone your fork.
2. Install the prerequisites listed in the [README](README.md#building).
3. Confirm the baseline builds and tests pass:

   ```sh
   cargo test --workspace
   cd apps/editor && npm install && npm run build
   ```

## Where things live

- **Platform-independent logic** belongs in `crates/zones-core`. It must not call OS APIs.
- **OS code** belongs in the matching backend crate (`zones-win`, `zones-linux`,
  `zones-archon-bridge`) and should implement the traits in `zones-core::backend`.
- **UI code** belongs in `apps/editor`. It talks to the core only through Tauri commands.

Read [`docs/architecture.md`](docs/architecture.md) before changing a trait, the config
format or the IPC protocol. Those are shared contracts and changes to them need
discussion first.

## Workflow

1. Open an issue (or comment on an existing one) before starting large work.
2. Create a branch from `master`: `feat/<short-name>`, `fix/<short-name>` or `docs/<short-name>`.
3. Keep commits focused. Write messages in the imperative mood ("Add hotkey parser").
4. Open a pull request using the template. Link the issue and the `todo.md` item if one applies.
5. Make sure CI is green.

## Code standards

### Rust

- Format with `cargo fmt`. Configuration is in `rustfmt.toml`.
- Lint with `cargo clippy --workspace --all-targets -- -D warnings`. Configuration is in `clippy.toml`.
- No `unsafe` in `zones-core`. Other crates may use `unsafe` only at FFI boundaries, with a
  `// SAFETY:` comment on every block.
- Public items need doc comments (`missing_docs` is enabled).
- Add unit tests for pure logic. Mark tests that need a real desktop session with `#[ignore]`
  and document how to run them.

### TypeScript

- Format with Prettier and lint with ESLint: `npm run format` and `npm run lint` in `apps/editor`.
- Strict TypeScript. Do not use `any` without a comment explaining why.

### Commits and licensing

By contributing you agree that your contribution is licensed under the MIT and Apache-2.0
terms of this project, as described in the README.

## Reporting bugs and security issues

- Use the bug report template for ordinary bugs.
- For security issues, especially in hotkey or input hooking and the IPC surface, do **not**
  open a public issue. Contact TPT Solutions privately through the repository's security
  advisory form, or by email if no form is available.
