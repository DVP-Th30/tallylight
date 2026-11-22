# Contributing

Thanks for looking. tallylight is young and small; the most useful contributions right now are:

- **Trying it and reporting what broke**, with your OS, agent version and the output of `tallylight --version`.
- **A new agent.** The daemon turns an agent's events into the five states. `crates/tallylight-core/src/claude.rs` is the Claude Code translation (a pure, tested function) and the model to follow; `codex.rs` next to it shows how a second agent reuses it.
- **A new screen.** See [docs/PROTOCOL.md](docs/PROTOCOL.md) and the client crate. Screens live in their own repositories; open an issue to get yours listed.

For anything larger than a fix, open an issue first so we agree on the direction. tallylight is a building block: features that turn it into a remote-control app for agents are out of scope.

## Working on the code

```sh
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs the same three commands. A daemon you start by hand for testing should use another port (`tallylight daemon --port 47796`) so it doesn't clash with your real one.

Code and identifiers are in English. Many comments and the design notes in `docs/` are still in French; English is welcome in new code and they will be translated over time. User-facing text is English.

Commits follow `type(scope): description` (`feat`, `fix`, `docs`, `chore`, `refactor`).

## License

By contributing you agree that your work is licensed under MIT OR Apache-2.0, like the rest of the project.
