# Security policy

## Supported versions

Only the latest release gets security fixes.

## Reporting a vulnerability

Please **don't open a public issue** for security problems. Use GitHub's private reporting instead: the **Security** tab of this repository → **Report a vulnerability**.

Include what you found, how to reproduce it, and the impact you expect. You should get a reply within 7 days. Once a fix is released you'll be credited unless you'd rather not be.

## What Super Folder does and doesn't do

Useful context when assessing a report:

- It makes **no network connections**. There's no update check, telemetry, crash reporting or cloud feature.
- It only reads and moves files in the folders you choose (the Super Folder, watch folders, and folders you search or scan). It never deletes files.
- Settings, rules and history are plain JSON in the per-user config folder (see the README).
- The window uses a strict Content Security Policy and only the Tauri permissions it needs (folder pickers and dialogs). File operations happen in Rust commands, not in the web view.
- Imported rule files are validated and size-limited. A rule can only move files to a destination you can see in the preview.

Particularly interesting areas: path handling (moving outside the intended folders, symlink tricks, `..` in rule destinations), anything that could cause data loss, and the rules import.
