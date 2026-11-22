#!/bin/sh
# Essai de bout en bout d'un binaire installé : aide, hooks, démon, un événement, appairage.
# Usage : smoke.sh <chemin du binaire>. N'utilise que le port et le dossier d'essai, jamais ceux de la machine.
set -eu

bin="$1"
port=47796
work="$(mktemp -d)"
export HOME="$work/home" XDG_DATA_HOME="$work/data" TALLYLIGHT_PORT="$port"
mkdir -p "$HOME"

"$bin" --version
"$bin" --help >/dev/null
"$bin" hooks --port "$port" | grep -q "127.0.0.1:$port/hooks/claude"
"$bin" install --no-service
grep -q "hooks/claude" "$HOME/.claude/settings.json"

"$bin" daemon --port "$port" >"$work/daemon.log" 2>&1 &
daemon=$!
trap 'kill "$daemon" 2>/dev/null || true; cat "$work/daemon.log"; rm -rf "$work"' EXIT

tries=0
until curl -fsS "http://127.0.0.1:$port/sessions" >/dev/null 2>&1; do
  tries=$((tries + 1))
  [ "$tries" -lt 50 ] || { echo "the daemon did not open its port" >&2; exit 1; }
  sleep 0.2
done

curl -fsS -X POST "http://127.0.0.1:$port/hooks/claude" -H 'content-type: application/json' \
  -d '{"hook_event_name":"UserPromptSubmit","session_id":"smoke","cwd":"/tmp/demo","prompt":"hi"}' >/dev/null
state="$(curl -fsS "http://127.0.0.1:$port/sessions")"
echo "$state"
echo "$state" | grep -q '"working"'

# L'appairage demande le réseau : c'est la preuve que la machine rejoint un relais
"$bin" pair | tee "$work/pair.txt"
grep -q '#' "$work/pair.txt"
"$bin" devices
echo "smoke test passed"
