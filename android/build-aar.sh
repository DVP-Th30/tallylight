#!/usr/bin/env bash
# Construit le paquet Android de tallylight : un fichier AAR qui contient la bibliothèque native
# (`libtallylight_ffi.so`, profil `embed`), les liaisons Kotlin générées par UniFFI déjà compilées,
# l'objet `dev.tallylight.Android`, les permissions et les règles R8. Voir docs/INFRA.md.
#
# Usage : android/build-aar.sh [--dry-run] [--stub-native]
#   --dry-run      affiche les commandes sans rien compiler ni écrire
#   --stub-native  remplace la compilation Android par une fausse bibliothèque (machine sans NDK) :
#                  vérifie tout le reste de la chaîne ; l'AAR produit (suffixe -stub) est inutilisable
#
# Variables (valeurs par défaut entre parenthèses) :
#   ANDROID_NDK_HOME  NDK Android, r28 ou plus (ANDROID_NDK_ROOT, sinon le NDK le plus récent du SDK)
#   ANDROID_HOME      SDK Android, pour android.jar (ANDROID_SDK_ROOT)
#   ANDROID_JAR       android.jar contre lequel compiler le Kotlin (plateforme la plus récente du SDK)
#   ABIS              processeurs visés ("arm64-v8a x86_64")
#   MIN_SDK           version d'Android minimale, pour l'éditeur de liens et le manifeste (26)
#   KOTLINC           compilateur Kotlin (kotlinc)
#   JNA_JAR           jar de JNA pour la compilation (téléchargé depuis Maven Central si absent)
#   OUT_DIR           dossier de sortie (target/android)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
HERE="$ROOT/android"
DRY_RUN=""
STUB=""
for arg in "$@"; do
    case "$arg" in
        --dry-run) DRY_RUN=1 ;;
        --stub-native) STUB=1 ;;
        -h | --help) sed -n '2,21p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "argument inconnu : $arg" >&2; exit 2 ;;
    esac
done

ABIS="${ABIS:-arm64-v8a x86_64}"
MIN_SDK="${MIN_SDK:-26}"
KOTLINC="${KOTLINC:-kotlinc}"
OUT_DIR="${OUT_DIR:-$ROOT/target/android}"
# Version de JNA contre laquelle on compile ; l'application choisit la sienne (5.x)
JNA_VERSION="5.19.1"
JNA_SHA256="4fb141dd8ef6b0585ffceea4bc49602fbc6312fa977e2c488794ea3e6aafecae"

say() { echo "==> $*"; }
fail() { echo "erreur : $*" >&2; exit 1; }
# Exécute une commande, ou l'affiche seulement en mode à blanc
run() {
    echo "+ $*"
    [ -n "$DRY_RUN" ] || "$@"
}
# En mode à blanc, un outil manquant est signalé sans arrêter le script
need() {
    if [ -n "$DRY_RUN" ]; then echo "(à blanc) $*"; else fail "$*"; fi
}
sha256() {
    if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

cd "$ROOT"
VERSION="$(cargo pkgid -p tallylight-ffi | sed 's/.*[#@]//')"
NAME="tallylight-$VERSION${STUB:+-stub}"
STAGE="$OUT_DIR/stage"
say "tallylight $VERSION : ABIS=\"$ABIS\" MIN_SDK=$MIN_SDK${DRY_RUN:+ (à blanc)}${STUB:+ (fausse bibliothèque native)}"

case "$(uname -s)" in
    Linux) NDK_HOST=linux-x86_64; HOST_LIB=libtallylight_ffi.so ;;
    Darwin) NDK_HOST=darwin-x86_64; HOST_LIB=libtallylight_ffi.dylib ;;
    *) fail "système non pris en charge : $(uname -s)" ;;
esac

# --- Outils -------------------------------------------------------------------------------------
SDK="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
if [ -z "${ANDROID_JAR:-}" ] && [ -n "$SDK" ] && [ -d "$SDK/platforms" ]; then
    # Plateforme la plus récente installée (android-36, android-37.0…)
    ANDROID_JAR="$(ls -d "$SDK"/platforms/android-*/android.jar 2>/dev/null | sort -V | tail -n1 || true)"
fi
[ -n "${ANDROID_JAR:-}" ] && [ -f "$ANDROID_JAR" ] || need "android.jar introuvable : définir ANDROID_HOME ou ANDROID_JAR"
command -v "$KOTLINC" >/dev/null || need "compilateur Kotlin introuvable : installer kotlinc ou définir KOTLINC"
command -v jar >/dev/null || need "jar (JDK) introuvable"
command -v zip >/dev/null || need "zip introuvable"

if [ -z "$STUB" ]; then
    NDK="${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-}}"
    if [ -z "$NDK" ] && [ -n "$SDK" ] && [ -d "$SDK/ndk" ]; then
        NDK="$(ls -d "$SDK"/ndk/* 2>/dev/null | sort -V | tail -n1 || true)"
    fi
    [ -n "$NDK" ] && [ -d "$NDK" ] || need "NDK introuvable : définir ANDROID_NDK_HOME (ou --stub-native pour essayer sans)"
    TC="${NDK:-<ndk>}/toolchains/llvm/prebuilt/$NDK_HOST/bin"
fi

rust_target() {
    case "$1" in
        arm64-v8a) echo aarch64-linux-android ;;
        x86_64) echo x86_64-linux-android ;;
        armeabi-v7a) echo armv7-linux-androideabi ;;
        x86) echo i686-linux-android ;;
        *) fail "ABI inconnue : $1" ;;
    esac
}
# Préfixe du compilateur C du NDK (diffère de la cible Rust pour armv7)
clang_prefix() {
    case "$1" in
        armv7-linux-androideabi) echo armv7a-linux-androideabi ;;
        *) echo "$1" ;;
    esac
}

run rm -rf "$STAGE"
run mkdir -p "$STAGE/aar" "$STAGE/kotlin" "$STAGE/classes" "$OUT_DIR"

# --- 1. Bibliothèque native, une par processeur ---------------------------------------------------
for abi in $ABIS; do
    target="$(rust_target "$abi")"
    run mkdir -p "$STAGE/aar/jni/$abi"
    if [ -n "$STUB" ]; then
        say "$abi : fausse bibliothèque (--stub-native)"
        [ -n "$DRY_RUN" ] || printf 'fausse bibliothèque tallylight pour %s : AAR inutilisable\n' "$abi" \
            >"$STAGE/aar/jni/$abi/libtallylight_ffi.so"
        continue
    fi
    if command -v rustup >/dev/null && ! rustup target list --installed | grep -qx "$target"; then
        need "cible Rust absente : rustup target add $target"
    fi
    say "$abi : compilation de tallylight-ffi (profil embed, cible $target)"
    upper="$(echo "$target" | tr 'a-z-' 'A-Z_')"
    lower="$(echo "$target" | tr '-' '_')"
    cc="$TC/$(clang_prefix "$target")$MIN_SDK-clang"
    # Pages de 16 Ko : exigées par Android 15 et plus sur certains téléphones, et par Google Play
    run env \
        "CARGO_TARGET_${upper}_LINKER=$cc" \
        "CARGO_TARGET_${upper}_RUSTFLAGS=-C link-arg=-Wl,-z,max-page-size=16384" \
        "CC_${lower}=$cc" "AR_${lower}=$TC/llvm-ar" \
        cargo build --locked --profile embed -p tallylight-ffi --lib --target "$target"
    so="target/$target/embed/libtallylight_ffi.so"
    run cp "$so" "$STAGE/aar/jni/$abi/"
    if [ -z "$DRY_RUN" ]; then
        # Aucune dépendance hors du système (libc++_shared.so devrait être livrée à part)
        needed="$("$TC/llvm-readelf" -d "$so" | sed -n 's/.*(NEEDED).*\[\(.*\)\]/\1/p' | tr '\n' ' ')"
        echo "    dépendances : $needed"
        for lib in $needed; do
            case "$lib" in
                libc.so | libm.so | libdl.so | liblog.so | libandroid.so) ;;
                *) fail "$abi : dépendance native inattendue $lib" ;;
            esac
        done
        for align in $("$TC/llvm-readelf" -lW "$so" | awk '$1 == "LOAD" { print $NF }'); do
            ((align % 16384 == 0)) || fail "$abi : segment aligné sur $align, pas sur 16 Ko"
        done
        echo "    taille : $(du -h "$so" | cut -f1), segments alignés sur 16 Ko"
    fi
done

# --- 2. Liaisons Kotlin, générées depuis la bibliothèque de la machine hôte ------------------------
say "liaisons Kotlin (UniFFI)"
run cargo build --locked -p tallylight-ffi -p tallylight-bindgen
run target/debug/tallylight-bindgen generate "target/debug/$HOST_LIB" \
    --language kotlin --no-format --out-dir "$STAGE/kotlin"
run cp -R "$HERE/src/." "$STAGE/kotlin/"

# --- 3. Compilation Kotlin ----------------------------------------------------------------------
if [ -z "${JNA_JAR:-}" ]; then
    JNA_JAR="$OUT_DIR/cache/jna-$JNA_VERSION.jar"
    if [ ! -f "$JNA_JAR" ]; then
        run mkdir -p "$OUT_DIR/cache"
        run curl -fsSL -o "$JNA_JAR.part" \
            "https://repo1.maven.org/maven2/net/java/dev/jna/jna/$JNA_VERSION/jna-$JNA_VERSION.jar"
        if [ -z "$DRY_RUN" ]; then
            [ "$(sha256 "$JNA_JAR.part")" = "$JNA_SHA256" ] || fail "empreinte de JNA inattendue"
            mv "$JNA_JAR.part" "$JNA_JAR"
        fi
    fi
fi
say "compilation Kotlin contre $(basename "$(dirname "${ANDROID_JAR:-?/?}")") et JNA"
# Contre android.jar seul (-no-jdk) : aucune API Java absente d'Android. Version de langage et
# bytecode bas : lisibles par les applications compilées avec un Kotlin plus ancien.
run "$KOTLINC" -no-jdk -no-reflect -nowarn \
    -classpath "${ANDROID_JAR:-android.jar}:$JNA_JAR" \
    -jvm-target 1.8 -language-version 2.1 -api-version 2.1 \
    -module-name tallylight -d "$STAGE/classes" "$STAGE/kotlin"
run jar cf "$STAGE/aar/classes.jar" -C "$STAGE/classes" .

# --- 4. Assemblage de l'AAR ---------------------------------------------------------------------
say "assemblage de $NAME.aar"
if [ -z "$DRY_RUN" ]; then
    sed "s/@MIN_SDK@/$MIN_SDK/" "$HERE/AndroidManifest.xml" >"$STAGE/aar/AndroidManifest.xml"
    : >"$STAGE/aar/R.txt"
fi
run cp "$HERE/proguard.txt" "$STAGE/aar/proguard.txt"
run rm -f "$OUT_DIR/$NAME.aar" "$OUT_DIR/$NAME-sources.jar"
run sh -c "cd '$STAGE/aar' && zip -qrX '$OUT_DIR/$NAME.aar' AndroidManifest.xml classes.jar R.txt proguard.txt jni"
run jar cf "$OUT_DIR/$NAME-sources.jar" -C "$STAGE/kotlin" .

if [ -z "$DRY_RUN" ]; then
    # Contrôle : les fichiers que Gradle attend sont présents
    listing="$(unzip -Z1 "$OUT_DIR/$NAME.aar")"
    for entry in AndroidManifest.xml classes.jar proguard.txt; do
        echo "$listing" | grep -qx "$entry" || fail "$entry manque dans l'AAR"
    done
    for abi in $ABIS; do
        echo "$listing" | grep -qx "jni/$abi/libtallylight_ffi.so" || fail "jni/$abi manque dans l'AAR"
    done
    unzip -Z1 "$STAGE/aar/classes.jar" | grep -qx 'dev/tallylight/Android.class' || fail "Android.class manque"
    unzip -Z1 "$STAGE/aar/classes.jar" | grep -qx 'uniffi/tallylight_ffi/Tallylight.class' || fail "Tallylight.class manque"
    say "prêt : $OUT_DIR/$NAME.aar ($(du -h "$OUT_DIR/$NAME.aar" | cut -f1)) et $NAME-sources.jar"
fi
