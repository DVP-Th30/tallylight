# Security

tallylight moves information about your coding sessions between your machines, so reports are taken seriously.

## Reporting

Please don't open a public issue for a vulnerability. Use GitHub's private reporting instead: the **Report a vulnerability** button under the repository's *Security* tab. You'll get an answer within a few days.

Only the latest release is supported.

## What to keep in mind

- A machine only serves screens on its allow-list; pairing needs a one-time code valid five minutes (see [docs/PROTOCOL.md](docs/PROTOCOL.md)).
- The daemon's local port (`127.0.0.1:47700`) trusts every process of the machine. A local user can read session state and request a pairing code. Shared machines are not a supported setup yet.
- The default relays are the public ones run by iroh's maintainers. They only see encrypted traffic, but for anything beyond personal use run your own (`--relay`).
- Session snapshots contain working directories, tool names, short descriptions and the start of the agent's last message. Pair only screens you trust.
