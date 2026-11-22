# Changelog

## 0.1.0

First public version.

- Daemon for Linux and macOS: follows Claude Code sessions through its hooks and session files, keeps them across restarts.
- States in the A2A `TaskState` vocabulary, with the tool in use, a one-line detail, the last message and the model.
- Pairing by one-time code and QR; allow-list of screens (`pair`, `devices`, `unpair`).
- Peer-to-peer, end-to-end encrypted transport (iroh); choice of relays (`--relay`, `TALLYLIGHT_RELAY`).
- `tallylight watch`: terminal screen that follows several machines and reconnects.
- `tallylight install` / `uninstall`: hooks and user service (systemd, launchd).
- Client library in Rust, Kotlin bindings (UniFFI) and an Android `.aar` build.
