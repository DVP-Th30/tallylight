#!/bin/sh
# Essai de bout en bout d'un binaire installé : aide, hooks, démon, un événement, appairage.
# Usage : smoke.sh <chemin du binaire>. N'utilise que le port et le dossier d'essai, jamais ceux de la machine.
set -eu

bin="$1"
port=47796
work="$(mktemp -d)"
export HOME="$work/home" XDG_DATA_HOME="$work/data" TALLYLIGHT_PORT="$port"
unset CODEX_HOME CLAUDE_CONFIG_DIR
mkdir -p "$HOME"

# Échoue si le motif est dans le fichier (`! grep` n'arrête pas un script sous `set -e`)
absent() {
  if grep -q "$1" "$2"; then
    echo "unexpected \"$1\" in $2" >&2
    exit 1
  fi
}

"$bin" --version
"$bin" --help >/dev/null
"$bin" hooks --port "$port" | grep -q "127.0.0.1:$port/hooks/claude"
"$bin" install --no-service
grep -q "hooks/claude" "$HOME/.claude/settings.json"

# Codex : absent de la version 0.1.0, que ce script essaie aussi (install-test)
codex=no
if "$bin" hooks codex --port "$port" >/dev/null 2>&1; then
  codex=yes
  "$bin" hooks codex --port "$port" | grep -q "hook codex --port $port"
  # Les hooks de Codex ne sont posés que si Codex est là : son dossier suffit à le dire
  mkdir "$HOME/.codex"
  printf '{\n  "description": "mine"\n}\n' >"$HOME/.codex/hooks.json"
  "$bin" install --no-service
  grep -q "hook codex --port $port" "$HOME/.codex/hooks.json"
  grep -q '"description": "mine"' "$HOME/.codex/hooks.json.before-tallylight"
fi

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

if [ "$codex" = yes ]; then
  # Codex lance la commande du hook et lui donne l'événement sur son entrée
  printf '%s' '{"session_id":"smoke-codex","turn_id":"t1","transcript_path":null,"cwd":"/tmp/api","hook_event_name":"PermissionRequest","model":"gpt-example","permission_mode":"default","tool_name":"Bash","tool_input":{"command":"rm -rf build","description":"Clean the build folder"}}' \
    | "$bin" hook codex --port "$port"
  state="$(curl -fsS "http://127.0.0.1:$port/sessions")"
  echo "$state"
  echo "$state" | grep -q '"agent":"codex"'
  echo "$state" | grep -q '"input-required"'
  echo "$state" | grep -q '"detail":"Clean the build folder"'
  # Démon injoignable ou entrée illisible : la commande ne dit rien et n'échoue pas
  printf 'not json' | "$bin" hook codex --port "$port"
  printf '{}' | "$bin" hook codex --port 1
  [ -z "$(printf '{}' | "$bin" hook codex --port 1 2>&1)" ]
fi

# L'appairage demande le réseau : c'est la preuve que la machine rejoint un relais
"$bin" pair | tee "$work/pair.txt"
grep -q '#' "$work/pair.txt"
"$bin" devices

if [ "$codex" = yes ]; then
  # Le retrait rend à Codex son fichier d'origine
  "$bin" uninstall
  absent "hook codex" "$HOME/.codex/hooks.json"
  grep -q '"description": "mine"' "$HOME/.codex/hooks.json"
  absent "hooks/claude" "$HOME/.claude/settings.json"
fi
echo "smoke test passed"
