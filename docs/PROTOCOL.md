# Protocol

What a tallylight machine and a screen exchange. Version 1, identified by the ALPN `tallylight/state/1`.

This is the contract for anyone writing a screen without the Rust client, or feeding states from an agent the daemon doesn't know yet. It can still change before 1.0; a breaking change will come with a new ALPN.

## Roles

- **Machine**: runs `tallylight daemon`. Collects agent events, keeps the list of sessions, serves paired screens.
- **Screen**: anything that displays the state. Connects to machines, never the other way round.

Both are [iroh](https://www.iroh.computer) endpoints: each has an Ed25519 key, and its public key is its identity. Connections are QUIC, encrypted end to end, direct when a path exists and relayed otherwise.

## Pairing

`tallylight pair` prints a pairing string, also shown as a QR code:

```
<ticket>#<code>
```

- `ticket`: an iroh endpoint ticket. It says where to reach the machine (public key, relay, addresses). It grants nothing by itself.
- `code`: 10 random bytes in hexadecimal, valid 5 minutes, usable once.

The machine keeps an allow-list of screen public keys. A screen that presents a valid code is added to it; from then on it connects with the ticket alone. `tallylight unpair` removes a key.

## Session

1. The screen opens a QUIC connection to the machine with ALPN `tallylight/state/1`, then one bidirectional stream.
2. The screen sends one line: `Hello`.
3. The machine answers one line: `Welcome`. If `ok` is false, the screen is not paired (unknown key and no valid code) and the machine closes.
4. The machine sends one `Snapshot` line immediately, then one per change, until either side closes.

Every message is one JSON object on one line, terminated by `\n`, UTF-8.

```json
{"name": "Kitchen tablet", "code": "9f2c41d07a3e5b6c8d10"}
```
```json
{"ok": true}
```
```json
{"machine": "build-server", "sessions": [ … ]}
```

`Hello.name` is shown by `tallylight devices`. `Hello.code` is omitted or `null` once paired.

## Snapshot

A snapshot is the full list of sessions on the machine, not a diff. A session that ended is simply absent from the next snapshot.

| Field | Type | |
|---|---|---|
| `id` | string | Stable for the life of the session. |
| `agent` | string | `claude-code` today. |
| `cwd` | string | Working directory. Its last component is the default display name. |
| `title` | string or null | Name given to the session, if any. |
| `state` | string or null | See below. `null` until the first turn starts. |
| `tool` | string or null | Tool running, or tool waiting for a permission. |
| `detail` | string or null | What the tool does, one line, 140 characters at most. |
| `last_message` | string or null | First 600 characters of the agent's last message, set when a turn ends, cleared when the next one starts. |
| `url` | string or null | Opens the session in the agent's own app, when there is one. |
| `model` | string or null | Identifier of the model that answered last, for example `claude-opus-5-5`. |
| `updated_at` | integer | Unix seconds of the last state or tool change. |

Unknown fields must be ignored: new ones may be added without a new protocol version.

## States

The values of `state` follow `TaskState` from A2A 1.0.

| Value | Meaning |
|---|---|
| `working` | The agent is thinking or running a tool. |
| `input-required` | The agent waits for an answer or a permission. |
| `completed` | The turn is finished. |
| `failed` | The turn ended on an error. |
| `canceled` | The turn was interrupted. |

## Local HTTP

The daemon also listens on `127.0.0.1:47700` (change with `--port`):

- `GET /sessions` returns the current snapshot.
- `POST /hooks/claude` receives Claude Code hook events (the hook's own JSON body).

This port trusts every local process; it is not reachable from the network.
