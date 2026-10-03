# Security policy

## Supported versions

Only the latest release gets security fixes.

## Reporting a vulnerability

Please **don't open a public issue** for security problems. Use GitHub's private reporting instead: the **Security** tab of this repository → **Report a vulnerability**.

Include what you found, how to reproduce it, and the impact you expect. You should get a reply within 7 days. Once a fix is released you'll be credited unless you'd rather not be.

## What DeskZero does and doesn't do

Useful context when assessing a report:

- It makes **no network connections** except the update check: when you press *Check for updates* (or weekly, if you turn that on) it fetches `latest.json` from this repository's GitHub releases. Updates are signed; the app refuses a package whose signature doesn't match the public key built into it. There's no telemetry, crash reporting or cloud feature.
- It only reads and moves files in the folders you choose (your DeskZero, watch folders, and folders you search or scan). Universal search reads file *names* in your usual folders to build an in-memory index; it never reads file contents and never stores the index on disk. It never deletes files.
- Settings, rules and history are plain JSON in the per-user config folder (see the README).
- The window uses a strict Content Security Policy and only the Tauri permissions it needs (folder pickers and dialogs). File operations happen in Rust commands, not in the web view.
- Imported rule files are validated and size-limited. A rule can only move files to a destination you can see in the preview.

Particularly interesting areas: the update path (signature checks), path handling (moving outside the intended folders, symlink tricks, `..` in rule destinations), anything that could cause data loss, and the rules import.
