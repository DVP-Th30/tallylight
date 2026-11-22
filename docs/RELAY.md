# Running your own relay

tallylight connects a machine and a screen directly whenever it can. A relay is only the meeting point: it helps both sides find each other, and carries the traffic when no direct path exists. It sees ciphertext only.

By default tallylight uses the public relays run by the maintainers of [iroh](https://www.iroh.computer). Their documentation describes them as "suitable for development and testing": rate-limited, shared, no uptime guarantee. For a team or a product, run your own. A relay is one small program, [`iroh-relay`](https://github.com/n0-computer/iroh/tree/main/iroh-relay).

## What you need

- A server reachable from the internet, with ports 80 and 443 open.
- A domain name pointing to it, for example `relay.example.org`. The relay gets its TLS certificate from Let's Encrypt by itself.

## Set it up

Configuration, in `/etc/iroh-relay/config.toml`:

```toml
http_bind_addr = "[::]:80"

[tls]
https_bind_addr = "[::]:443"
hostname = "relay.example.org"
cert_mode = "LetsEncrypt"
cert_dir = "/var/lib/iroh-relay/certs"
contact = "you@example.org"
```

Run it with the official Docker image, `n0computer/iroh-relay`, or with the binary from the iroh releases page:

```sh
iroh-relay -c /etc/iroh-relay/config.toml
```

Check that it answers:

```sh
curl --fail https://relay.example.org/healthz
```

By default the relay accepts everyone. To keep it to your own machines and screens, see the `access` options in the iroh-relay README (allow-list of endpoint identities, or a shared token).

## Point tallylight at it

On each machine:

```sh
tallylight install --relay https://relay.example.org
```

That is enough: the machine announces its relay in the pairing string, so screens reach it through your relay without any setting of their own. To make a screen use your relay for its own side too, pass `--relay` to `tallylight watch`, set `TALLYLIGHT_RELAY`, or use `Tallylight.withRelay(...)` in an app. Several relays can be given, separated by commas.

## What has been verified

tallylight was tested against a local `iroh-relay --dev` (version 1.4.1): pairing, sessions received through the relay, then a direct connection. The Let's Encrypt setup above comes from iroh's self-hosting guide, which is maintained by its community, and has not been run by the tallylight project yet. Match the relay version to the iroh version tallylight is built with (1.4 today).
