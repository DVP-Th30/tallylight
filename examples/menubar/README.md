# tallylight in the macOS menu bar

A plugin for [SwiftBar](https://github.com/swiftbar/SwiftBar) or [xbar](https://xbarapp.com). It reads the tallylight daemon of the same machine over `http://127.0.0.1:47700/sessions`: no pairing, no library.

- `●` in red with a number: that many agents are waiting for you.
- `◐` with a number: agents at work.
- `○`: nothing running.

The menu lists every session with its state and, when there is one, what it is doing or asking.

## Install

1. Install tallylight and run `tallylight install`.
2. Install SwiftBar or xbar.
3. Copy `tallylight.5s.sh` into the plugin folder and make sure it is executable (`chmod +x`).

The `5s` in the file name is the refresh interval. The script needs `curl` and `jq`; both ship with macOS 15 and later (`brew install jq` on older systems).

The same script is a starting point for any status bar on Linux (Waybar, Polybar, Argos): keep the `curl` and the `jq` filter, change the output format.
