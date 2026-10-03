# Contributing

Thanks for helping. Super Folder aims to stay **small, offline and simple**, so please read the scope below before starting on a feature.

## Scope

In scope: sorting files reliably and safely, clear previews, undo, cross-platform polish, performance, accessibility, and bug fixes.

Out of scope (pull requests will be declined): AI/LLM features, cloud sync, accounts, telemetry, servers or web APIs, plugin systems, reports/analytics, and anything that deletes files automatically.

If you're unsure, open an issue first.

## Setup

1. Install [Rust](https://rustup.rs), Node.js 20+, and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).
2. `npm install`
3. `npm run tauri dev`

## Before opening a pull request

```bash
npm run typecheck
cd src-tauri
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
```

CI runs the same checks on Windows, macOS and Linux.

## Guidelines

- **Logic belongs in `sf-engine`.** It's pure Rust with no Tauri dependency, so it can be unit-tested against temporary folders. The Tauri commands in `src-tauri/src/` should stay thin adapters.
- **Plan first, then execute.** Anything that moves files must be previewable: planning never touches the disk.
- **Never lose data.** No deletes, no silent overwrites, and every move must be recorded so it can be undone.
- **Add a test** for any engine behaviour you change, especially edge cases (Unicode names, conflicts, symlinks, locked files).
- **Few dependencies.** Explain why a new crate or npm package is needed.
- Keep the UI plain and native-feeling. No UI kits or state libraries.

## Reporting bugs

Include your OS and version, the Super Folder version, what you did, what happened, and what you expected. Your `history.jsonl` (see the README for its location) helps, but check it for private file names before sharing.
