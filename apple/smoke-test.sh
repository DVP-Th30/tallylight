#!/usr/bin/env bash
# Essai des liaisons Swift, de bout en bout : lance un démon d'essai, lui envoie un événement d'agent,
# demande une chaîne d'appairage, puis fait tourner `apple/smoke` (un programme Swift qui utilise les
# liaisons) jusqu'à ce qu'il reçoive la session. À lancer après `apple/build-xcframework.sh`.
# N'utilise que le port et les dossiers d'essai, jamais ceux de la machine.
#
# Usage : apple/smoke-test.sh [macos | ios-link | ios-simulator]
#   macos          (défaut) le programme est construit par SwiftPM à partir du paquet `apple/`
#   ios-link       édition de liens du programme pour iPhone, sans le lancer (Xcode complet)
#   ios-simulator  le programme tourne dans un simulateur iOS (Xcode complet)
# Variables :
#   TALLYLIGHT_BIN  binaire du démon d'essai (compilé par `cargo build -p tallylight` si absent)
#   SIMULATOR       identifiant du simulateur (le premier iPhone disponible)
#   IOS_MIN         version d'iOS visée (15.0)
#   TEST_PORT       port du démon d'essai (47796) ; jamais 47700, celui du démon de la machine
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MODE="${1:-macos}"
PORT="${TEST_PORT:-47796}"
SESSION=swift-smoke
IOS_MIN="${IOS_MIN:-15.0}"
FRAMEWORK=apple/TallylightFFI.xcframework
cd "$ROOT"

fail() { echo "erreur : $*" >&2; exit 1; }

case "$MODE" in
    macos | ios-link | ios-simulator) ;;
    -h | --help) sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) fail "mode inconnu : $MODE" ;;
esac
[ -d "$FRAMEWORK" ] || fail "lancer d'abord apple/build-xcframework.sh"
[ "$PORT" != 47700 ] || fail "47700 est le port du démon de la machine : choisir un autre TEST_PORT"

work="$(mktemp -d)"
daemon=""
finish() {
    if [ -n "$daemon" ]; then
        kill "$daemon" 2>/dev/null || true
        wait "$daemon" 2>/dev/null || true
        echo "--- journal du démon d'essai"
        cat "$work/daemon.log"
    fi
    rm -rf "$work"
}
trap finish EXIT

# Hors de SwiftPM (qui ne construit pas d'exécutable pour iOS) : les liaisons et le programme d'essai
# compilés ensemble, liés à une tranche du xcframework. $1 : plateforme et variante de la tranche,
# $2 : cible Swift, $3 : SDK, $4 : exécutable produit
build_ios() {
    local slice
    slice="$(ls -d "$FRAMEWORK"/$1 2>/dev/null | head -n1)"
    [ -n "$slice" ] || fail "pas de tranche $1 dans le xcframework"
    xcrun swiftc -target "$2" -sdk "$(xcrun --sdk "$3" --show-sdk-path)" \
        -I "$slice/Headers" -L "$slice" -ltallylight_ffi \
        -framework Security -framework SystemConfiguration -framework CoreFoundation -framework Network \
        apple/Sources/TallylightKit/Tallylight.swift apple/smoke/Sources/tallylight-smoke/main.swift \
        -o "$4"
    echo "lié pour $2 : $(du -h "$4" | cut -f1)"
}

case "$MODE" in
    macos) swift build --package-path apple/smoke ;;
    ios-link)
        build_ios 'ios-arm64' "arm64-apple-ios$IOS_MIN" iphoneos "$work/tallylight-smoke"
        echo "édition de liens pour iPhone réussie"
        exit 0
        ;;
    ios-simulator)
        build_ios 'ios-arm64*-simulator' "arm64-apple-ios$IOS_MIN-simulator" iphonesimulator "$work/tallylight-smoke"
        SIMULATOR="${SIMULATOR:-$(xcrun simctl list devices available | grep -m1 iPhone | grep -oE '[0-9A-F-]{36}' || true)}"
        [ -n "$SIMULATOR" ] || fail "aucun simulateur d'iPhone disponible"
        # Démarre le simulateur s'il ne l'est pas, et attend qu'il soit prêt
        xcrun simctl bootstatus "$SIMULATOR" -b
        ;;
esac

if [ -z "${TALLYLIGHT_BIN:-}" ]; then
    cargo build --locked -p tallylight
    TALLYLIGHT_BIN="$ROOT/target/debug/tallylight"
fi

mkdir -p "$work/home" "$work/screen"
# Un démon déjà présent sur ce port répondrait à la place du nôtre
if curl -fsS "http://127.0.0.1:$PORT/sessions" >/dev/null 2>&1; then
    fail "le port $PORT est déjà pris par un autre démon : choisir un autre TEST_PORT"
fi
# Le démon d'essai a son propre dossier personnel : ni réglages ni données de la machine
daemon_env=(env "HOME=$work/home" "XDG_DATA_HOME=$work/data" "TALLYLIGHT_PORT=$PORT")
"${daemon_env[@]}" "$TALLYLIGHT_BIN" daemon --port "$PORT" >"$work/daemon.log" 2>&1 &
daemon=$!

tries=0
until curl -fsS "http://127.0.0.1:$PORT/sessions" >/dev/null 2>&1; do
    tries=$((tries + 1))
    [ "$tries" -lt 50 ] || fail "le démon d'essai n'a pas ouvert son port"
    sleep 0.2
done
kill -0 "$daemon" 2>/dev/null || fail "le démon d'essai s'est arrêté"

curl -fsS -X POST "http://127.0.0.1:$PORT/hooks/claude" -H 'content-type: application/json' \
    -d "{\"hook_event_name\":\"UserPromptSubmit\",\"session_id\":\"$SESSION\",\"cwd\":\"/tmp/demo\",\"prompt\":\"hi\"}" >/dev/null

# L'appairage demande le réseau (le démon doit avoir joint un relais)
pairing="$("${daemon_env[@]}" "$TALLYLIGHT_BIN" pair | sed -n "s/^ *tallylight watch '\(.*\)'$/\1/p")"
[ -n "$pairing" ] || fail "pas de chaîne d'appairage"

if [ "$MODE" = macos ]; then
    swift run --skip-build --package-path apple/smoke tallylight-smoke "$work/screen" "$pairing" "$SESSION"
else
    # Un processus du simulateur voit les fichiers et le réseau du Mac
    xcrun simctl spawn "$SIMULATOR" "$work/tallylight-smoke" "$work/screen" "$pairing" "$SESSION"
fi
"${daemon_env[@]}" "$TALLYLIGHT_BIN" devices | grep -q "Swift smoke test" \
    || fail "l'écran Swift n'est pas dans la liste du démon"
echo "essai Swift réussi ($MODE)"
