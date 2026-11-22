# Changelog

## Unreleased

## 0.1.1

- Windows (x86_64): the daemon and every command build and run; `tallylight install` registers a Task Scheduler task that starts the daemon at logon; data lives in `%LOCALAPPDATA%\tallylight`; releases gain a `.zip` for `x86_64-pc-windows-msvc` and a PowerShell installer (`install.ps1`). Checked on CI machines only so far.
- Sessions of a Windows machine show their folder name, and only the file name of the file being edited, on every screen (paths with `\` were sent whole).
- Codex CLI (OpenAI) as a second agent: `tallylight install` adds command hooks to `~/.codex/hooks.json` when Codex is on the machine (backup kept, other hooks untouched, removed by `uninstall`); sessions carry `"agent": "codex"`. States: `working`, `input-required`, `completed`, `canceled`. `failed` is not reported, because Codex has no hook for a turn that ends on an error. Checked against the real Codex CLI 0.171.0 driven by a local mock model; not verified against a session with a real OpenAI model. Linux and macOS only: on Windows the Codex hooks are skipped.
- `tallylight hooks [claude|codex]` prints the hooks of both agents, or one agent's JSON alone. New `tallylight hook codex`, the command Codex runs for each event.
- Local HTTP: `POST /hooks/codex`.
- Swift bindings (UniFFI) for iOS and macOS: `apple/build-xcframework.sh` builds a Swift package (`TallylightFFI.xcframework` and `TallylightKit`). Tested on macOS and in the iOS simulator, not yet tried in a real iOS app. Requires iroh 1.4.2 (1.4.1 does not compile for iOS).

## 0.1.0

First public version.

- Daemon for Linux and macOS: follows Claude Code sessions through its hooks and session files, keeps them across restarts.
- States in the A2A `TaskState` vocabulary, with the tool in use, a one-line detail, the last message and the model.
- Pairing by one-time code and QR; allow-list of screens (`pair`, `devices`, `unpair`).
- Peer-to-peer, end-to-end encrypted transport (iroh); choice of relays (`--relay`, `TALLYLIGHT_RELAY`).
- `tallylight watch`: terminal screen that follows several machines and reconnects.
- `tallylight install` / `uninstall`: hooks and user service (systemd, launchd).
- Client library in Rust, Kotlin bindings (UniFFI) and an Android `.aar` build.
