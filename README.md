# tallylight

**Know when it's your turn.** tallylight shows the state of every coding agent you run, on the screens you choose: working, waiting for you, done.

[![ci](https://github.com/DVP-Th30/tallylight/actions/workflows/ci.yml/badge.svg)](https://github.com/DVP-Th30/tallylight/actions/workflows/ci.yml)
![license](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)

You start an agent on a server, two more on your laptop, and go back to your own work. One of them has been waiting on a permission prompt for twenty minutes, and nothing told you.

tallylight is the missing signal. One small binary per machine follows your agent sessions and streams their state to the screens you pair with it (a terminal, a phone, your own app) over a direct, end-to-end encrypted connection. No account, no server in the middle.

On a television set, the tally light is the red lamp on top of the camera that is live. Same idea.

- **Free and open source.** MIT or Apache-2.0.
- **No account.** Machines pair directly with screens. Nobody else sees your sessions.
- **Works away from home.** Peer-to-peer through NAT ([iroh](https://www.iroh.computer)), with an encrypted relay as a fallback.
- **Nothing to change in how you work.** It listens to the hooks your agent already fires. No wrapper, no tmux, no special terminal.
- **A building block, not another app.** A standard state vocabulary ([A2A](https://a2a-protocol.org) `TaskState`) and a client library any app can embed.

Status: early, and used daily by its author. Claude Code on Linux, macOS and Windows, read-only. See [what works today](#what-works-today).

## Install

Linux and macOS (x86_64 and arm64):

```sh
curl -fsSL https://dvp-th30.github.io/tallylight/install.sh | sh
```

Windows (x86_64), in PowerShell, from the first release after 0.1.0:

```powershell
irm https://dvp-th30.github.io/tallylight/install.ps1 | iex
```

The script downloads the latest release, checks its SHA-256 and puts `tallylight` in `~/.local/bin` (on Windows: in `%LOCALAPPDATA%\tallylight\bin`, which it adds to your user `PATH`). You can also grab a binary from the [releases page](https://github.com/DVP-Th30/tallylight/releases), or build it:

```sh
cargo install --git https://github.com/DVP-Th30/tallylight tallylight
```

## Quick start

On each machine where agents run:

```sh
tallylight install
```

This adds tallylight's hooks to your Claude Code settings (a backup is kept next to the file, other tools' hooks are left alone) and starts the daemon as a user service (systemd on Linux, launchd on macOS, a Task Scheduler task at logon on Windows). Sessions that are already open show up too.

Pair a screen:

```sh
tallylight pair
```

It prints a QR code and a pairing string. The code works once and expires after five minutes.

Watch from any terminal, on the same machine or another one:

```sh
tallylight watch '<pairing string>'
```

After the first time, `tallylight watch` alone follows every machine this terminal has paired with, and reconnects on its own.

Other commands: `tallylight devices` lists paired screens, `tallylight unpair <id>` removes one, `tallylight name <name>` sets how the machine is called on screens, `tallylight uninstall` removes the hooks and the service. `tallylight --help` has the rest.

## What a screen receives

One JSON snapshot per change, per machine:

```json
{
  "machine": "build-server",
  "sessions": [
    {
      "id": "6ab539da-2314-4416-9405-f1a20643455a",
      "agent": "claude-code",
      "cwd": "/home/me/billing-api",
      "title": "billing-api",
      "state": "input-required",
      "tool": "Bash",
      "detail": "Run the database migration",
      "last_message": null,
      "url": null,
      "model": "claude-opus-5-5",
      "updated_at": 1795298089
    }
  ]
}
```

| State | Meaning | Claude Code hooks |
|---|---|---|
| `working` | Thinking or running a tool | `UserPromptSubmit`, `PreToolUse`, `PostToolUse` |
| `input-required` | Waiting for your answer or permission | `PermissionRequest`, `Notification` |
| `completed` | Turn finished | `Stop` |
| `failed` | Turn ended on an error | `StopFailure` |
| `canceled` | Interrupted before the end | inferred from the session file |

`state` is `null` until the session's first turn. The wire protocol is described in [docs/PROTOCOL.md](docs/PROTOCOL.md).

## Put it in your own app

The client is one Rust crate, `tallylight-client`:

```rust
use tallylight_client::{Screen, generate_key};

let screen = Screen::new(generate_key(), "Kitchen tablet").await?;
let mut watch = screen.watch(pairing).await?;
while let Some(snapshot) = watch.next().await? {
    for session in snapshot.sessions.iter().filter(|s| s.state.is_some_and(|s| s.needs_user())) {
        println!("{} · {} needs you", snapshot.machine, session.label());
    }
}
```

Keep the key: it is the screen's identity, and the machine recognises it without a new code. Keep `watch.ticket()` too, and pass it to `watch()` next time instead of the pairing string.

For Android, `tallylight-ffi` wraps the client with [UniFFI](https://mozilla.github.io/uniffi-rs/) and handles storage, several machines and reconnection:

```kotlin
val tally = Tallylight(dataDir, "Kitchen tablet")
tally.start(object : Listener {
    override fun onMachine(machine: Machine) {
        machine.sessions
            .filter { it.state == "input-required" }
            .forEach { show("${machine.name} · ${it.title ?: it.project} needs you") }
    }
    override fun onMachineRemoved(id: String) {}
})
tally.pair(scannedQr)
```

`android/build-aar.sh` builds a ready-to-use `.aar` (native library, Kotlin bindings, permissions, R8 rules). The compressed library weighs about 3.4 MB. Swift bindings are next.

Building with an LLM? Point it at [llms.txt](https://dvp-th30.github.io/tallylight/llms.txt): the protocol, the states, the code and the limits in one file.

## Privacy and security

- **Pairing.** Each screen has its own key. To get on a machine's allow-list, it presents a random one-time code (valid five minutes, single use) shown by `tallylight pair`. After that the machine recognises the key. `tallylight unpair` removes a screen.
- **Transport.** QUIC, end-to-end encrypted between the machine and the screen. A relay is used only when no direct path is found, and it only sees ciphertext.
- **What leaves the machine.** Session state, working directory and title, the tool name, a one-line detail (at most 140 characters: a command's description, a file name, the question asked), the first 600 characters of the agent's last message, and the model name. Never file contents, never command output. A command without a description is reduced to the program's name.
- **Relays.** By default tallylight uses the public relays run by iroh's maintainers, which are meant for development. Use your own with `--relay https://relay.example.com` or `TALLYLIGHT_RELAY`.
- **Local port.** The daemon listens on `127.0.0.1:47700` and trusts local processes. Don't run it on a machine you share with people you don't trust.

Found a security problem? See [SECURITY.md](SECURITY.md).

## What works today

| | |
|---|---|
| Agents | Claude Code. Codex, Copilot CLI, Cursor and OpenCode are next and not tested yet. |
| Machines | Linux and macOS. Windows (x86_64) is new: see [Windows](#windows). |
| Screens | Terminal (`tallylight watch`), Android through the Kotlin bindings, any Rust program. |
| Direction | Read-only: screens show state, they can't answer the agent yet. |
| Network | Tested on the same Wi-Fi (8–13 ms, direct), across an office firewall and on 5G (direct after a few seconds through a relay). |

Known limits: a phone that is asleep is not woken up (no push notifications yet), and Claude Code logs a non-blocking hook error in its transcript when the daemon is stopped.

### Windows

Windows support is new and has only been exercised on CI machines, not yet on a real desktop with live Claude Code sessions. What to know:

- `tallylight install` writes the hooks to `%USERPROFILE%\.claude\settings.json` and registers a Task Scheduler task named `tallylight` that starts the daemon when you log on, with no window. Its log is `%LOCALAPPDATA%\tallylight\daemon.log`; data (identity, paired screens) lives in `%LOCALAPPDATA%\tallylight`.
- If the task can't be created on your machine, use `tallylight install --no-service` and run `tallylight daemon` yourself.
- Not checked yet on a real machine: session files and transcripts written by Claude Code on Windows (they feed already-open sessions, dead sessions and the model name), creating the task from an account without administrator rights, and Windows on ARM (the x86_64 build is installed there).
- A console window may flash when you log on, before the daemon detaches from it.
- Data files are not locked down beyond the default permissions of `%LOCALAPPDATA%` (your account, the system and administrators).

Reports are welcome.

## Roadmap

- **Next:** more agents; Swift bindings and an iOS screen; a packaged self-hosted relay.
- **Later:** a small Bluetooth desk display (ESP32); an optional wake-up service for sleeping phones, run by each app with its own keys; answering a permission request from the screen.

## Build from source

Rust stable.

```sh
cargo build --release -p tallylight
cargo test --workspace
```

The design notes in `docs/` (architecture, decisions, state of the art) are in French for now; [docs/PROTOCOL.md](docs/PROTOCOL.md) and this README are the English entry points. Contributions are welcome: see [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT), at your option.
