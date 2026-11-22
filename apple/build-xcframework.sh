#!/usr/bin/env bash
# Construit le paquet Swift de tallylight pour les plateformes Apple : `TallylightFFI.xcframework`
# (la bibliothèque statique `libtallylight_ffi.a`, profil `embed`, une tranche par plateforme, avec
# l'en-tête C généré par UniFFI) et `Sources/TallylightKit/Tallylight.swift` (les liaisons Swift).
# Les deux sont déposés dans `apple/`, à côté de `Package.swift` : le dossier devient un paquet Swift
# qu'une application ajoute tel quel. Une archive du paquet est aussi produite. Voir docs/INFRA.md.
#
# Usage : apple/build-xcframework.sh [--dry-run]
#   --dry-run  affiche les commandes sans rien compiler ni écrire
#
# Variables (valeurs par défaut entre parenthèses) ; une liste de cibles vide retire la plateforme :
#   IOS_TARGETS      cibles Rust pour iPhone et iPad ("aarch64-apple-ios")
#   IOS_SIM_TARGETS  cibles Rust pour le simulateur ("aarch64-apple-ios-sim" ; ajouter
#                    "x86_64-apple-ios" pour les simulateurs des Mac Intel)
#   MACOS_TARGETS    cibles Rust pour macOS ("aarch64-apple-darwin x86_64-apple-darwin")
#   IOS_MIN          version d'iOS minimale (15.0, comme dans Package.swift)
#   MACOS_MIN        version de macOS minimale (12.0, comme dans Package.swift)
#   PACKAGE_DIR      dossier du paquet Swift à remplir (apple/)
#   OUT_DIR          dossier de travail et de l'archive (target/apple)
#
# Les cibles iOS demandent Xcode complet (SDK iphoneos et iphonesimulator). Avec les seuls outils en
# ligne de commande, construire pour macOS : IOS_TARGETS= IOS_SIM_TARGETS= apple/build-xcframework.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DRY_RUN=""
for arg in "$@"; do
    case "$arg" in
        --dry-run) DRY_RUN=1 ;;
        -h | --help) sed -n '2,22p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "argument inconnu : $arg" >&2; exit 2 ;;
    esac
done

IOS_TARGETS="${IOS_TARGETS-aarch64-apple-ios}"
IOS_SIM_TARGETS="${IOS_SIM_TARGETS-aarch64-apple-ios-sim}"
MACOS_TARGETS="${MACOS_TARGETS-aarch64-apple-darwin x86_64-apple-darwin}"
IOS_MIN="${IOS_MIN:-15.0}"
MACOS_MIN="${MACOS_MIN:-12.0}"
PACKAGE_DIR="${PACKAGE_DIR:-$ROOT/apple}"
OUT_DIR="${OUT_DIR:-$ROOT/target/apple}"
LIB=libtallylight_ffi.a
FRAMEWORK=TallylightFFI.xcframework

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

[ "$(uname -s)" = Darwin ] || need "ce script demande macOS (lipo, SDK Apple)"
[ -n "$IOS_TARGETS$IOS_SIM_TARGETS$MACOS_TARGETS" ] || fail "aucune cible : IOS_TARGETS, IOS_SIM_TARGETS et MACOS_TARGETS sont vides"

cd "$ROOT"
VERSION="$(cargo pkgid -p tallylight-ffi | sed 's/.*[#@]//')"
STAGE="$OUT_DIR/stage"
say "tallylight $VERSION : iOS=\"$IOS_TARGETS\" simulateur=\"$IOS_SIM_TARGETS\" macOS=\"$MACOS_TARGETS\"${DRY_RUN:+ (à blanc)}"

# --- Outils -------------------------------------------------------------------------------------
command -v lipo >/dev/null || need "lipo introuvable : installer les outils en ligne de commande (xcode-select --install)"
command -v zip >/dev/null || need "zip introuvable"
# Xcode complet assemble le xcframework ; sans lui (outils en ligne de commande seuls), le script
# écrit lui-même le dossier et son Info.plist, ce qui suffit pour macOS
XCODEBUILD=""
if xcodebuild -version >/dev/null 2>&1; then XCODEBUILD=1; fi
if [ -n "$IOS_TARGETS" ]; then
    xcrun --sdk iphoneos --show-sdk-path >/dev/null 2>&1 \
        || need "SDK iphoneos introuvable : installer Xcode, ou construire sans iOS (IOS_TARGETS= IOS_SIM_TARGETS=)"
fi
if [ -n "$IOS_SIM_TARGETS" ]; then
    xcrun --sdk iphonesimulator --show-sdk-path >/dev/null 2>&1 \
        || need "SDK iphonesimulator introuvable : installer Xcode, ou construire sans simulateur (IOS_SIM_TARGETS=)"
fi
for target in $IOS_TARGETS $IOS_SIM_TARGETS $MACOS_TARGETS; do
    if command -v rustup >/dev/null && ! rustup target list --installed | grep -qx "$target"; then
        need "cible Rust absente : rustup target add $target"
    fi
done

# Architecture d'une cible Rust, telle que la nomment lipo et le xcframework
arch_of() {
    case "$1" in
        aarch64-*) echo arm64 ;;
        x86_64-*) echo x86_64 ;;
        *) fail "cible inconnue : $1" ;;
    esac
}

run rm -rf "$STAGE"
run mkdir -p "$STAGE/swift" "$STAGE/headers/TallylightFFI" "$OUT_DIR"

# --- 1. Bibliothèque statique : une par cible, puis une par plateforme ------------------------------
# Tranches du xcframework, une ligne par plateforme : « dossier|plateforme|variante|architectures »
SLICES=""
build_platform() {
    local platform="$1" variant="$2" targets="$3" target archs="" inputs=""
    [ -n "$targets" ] || return 0
    for target in $targets; do
        say "$platform${variant:+ ($variant)} : compilation de tallylight-ffi (profil embed, cible $target)"
        # `cargo rustc --crate-type` : la bibliothèque statique n'est produite qu'ici, sans alourdir
        # les autres compilations du crate (Android, tests). `native-static-libs` affiche ce que
        # l'application devra lier avec elle : à tenir à jour dans Package.swift
        run env "IPHONEOS_DEPLOYMENT_TARGET=$IOS_MIN" "MACOSX_DEPLOYMENT_TARGET=$MACOS_MIN" \
            cargo rustc --locked --profile embed -p tallylight-ffi --lib --crate-type staticlib --target "$target" \
            -- --print native-static-libs
        archs="$archs $(arch_of "$target")"
        inputs="$inputs target/$target/embed/$LIB"
    done
    archs="$(echo $archs | tr ' ' '\n' | sort | tr '\n' ' ' | sed 's/ $//')"
    local dir="$platform-$(echo "$archs" | tr ' ' '_')${variant:+-$variant}"
    run mkdir -p "$STAGE/lib/$dir"
    # Plusieurs architectures d'une même plateforme tiennent dans un seul fichier
    # shellcheck disable=SC2086
    run lipo -create $inputs -output "$STAGE/lib/$dir/$LIB"
    if [ -z "$DRY_RUN" ]; then
        echo "    $(lipo -info "$STAGE/lib/$dir/$LIB" | sed 's/.*are: //; s/.*architecture: //') : $(du -h "$STAGE/lib/$dir/$LIB" | cut -f1)"
    fi
    SLICES="$SLICES$dir|$platform|$variant|$archs
"
}
build_platform ios "" "$IOS_TARGETS"
build_platform ios simulator "$IOS_SIM_TARGETS"
build_platform macos "" "$MACOS_TARGETS"

# --- 2. Liaisons Swift, générées depuis la bibliothèque de la machine hôte --------------------------
say "liaisons Swift (UniFFI)"
run cargo build --locked -p tallylight-ffi -p tallylight-bindgen
run target/debug/tallylight-bindgen generate target/debug/libtallylight_ffi.dylib \
    --language swift --no-format --out-dir "$STAGE/swift"
# L'en-tête et sa carte de module dans un sous-dossier au nom du module : deux xcframeworks qui
# poseraient chacun un `module.modulemap` à la racine de leurs en-têtes se heurtent dans Xcode
run cp "$STAGE/swift/TallylightFFI.h" "$STAGE/headers/TallylightFFI/"
run cp "$STAGE/swift/TallylightFFI.modulemap" "$STAGE/headers/TallylightFFI/module.modulemap"

# --- 3. Assemblage du xcframework, dans une copie du paquet Swift ------------------------------------
say "assemblage de $FRAMEWORK"
PACKAGE_STAGE="$STAGE/Tallylight"
run mkdir -p "$PACKAGE_STAGE/Sources/TallylightKit"
run cp "$ROOT/apple/Package.swift" "$ROOT/LICENSE-MIT" "$ROOT/LICENSE-APACHE" "$PACKAGE_STAGE/"
run cp "$STAGE/swift/Tallylight.swift" "$PACKAGE_STAGE/Sources/TallylightKit/"
if [ -n "$XCODEBUILD" ]; then
    args=()
    while IFS='|' read -r dir _; do
        [ -n "$dir" ] || continue
        args+=(-library "$STAGE/lib/$dir/$LIB" -headers "$STAGE/headers")
    done <<EOF
$SLICES
EOF
    run xcodebuild -create-xcframework "${args[@]}" -output "$PACKAGE_STAGE/$FRAMEWORK"
else
    # Même disposition que celle de xcodebuild : un dossier par tranche, et un Info.plist qui les décrit
    say "xcodebuild absent : dossier et Info.plist écrits par le script"
    entries=""
    while IFS='|' read -r dir platform variant archs; do
        [ -n "$dir" ] || continue
        run mkdir -p "$PACKAGE_STAGE/$FRAMEWORK/$dir"
        run cp "$STAGE/lib/$dir/$LIB" "$PACKAGE_STAGE/$FRAMEWORK/$dir/"
        run cp -R "$STAGE/headers" "$PACKAGE_STAGE/$FRAMEWORK/$dir/Headers"
        arch_lines=""
        for arch in $archs; do arch_lines="$arch_lines
				<string>$arch</string>"; done
        entries="$entries
		<dict>
			<key>BinaryPath</key>
			<string>$LIB</string>
			<key>HeadersPath</key>
			<string>Headers</string>
			<key>LibraryIdentifier</key>
			<string>$dir</string>
			<key>LibraryPath</key>
			<string>$LIB</string>
			<key>SupportedArchitectures</key>
			<array>$arch_lines
			</array>
			<key>SupportedPlatform</key>
			<string>$platform</string>${variant:+
			<key>SupportedPlatformVariant</key>
			<string>$variant</string>}
		</dict>"
    done <<EOF
$SLICES
EOF
    if [ -z "$DRY_RUN" ]; then
        cat >"$PACKAGE_STAGE/$FRAMEWORK/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>AvailableLibraries</key>
	<array>$entries
	</array>
	<key>CFBundlePackageType</key>
	<string>XFWK</string>
	<key>XCFrameworkFormatVersion</key>
	<string>1.0</string>
</dict>
</plist>
EOF
    fi
fi

# --- 4. Archive, puis dépôt dans le paquet Swift ----------------------------------------------------
# L'archive contient le paquet entier (manifeste, liaisons, xcframework) : à décompresser puis à
# ajouter comme paquet local
ZIP="$OUT_DIR/tallylight-swift-$VERSION.zip"
run rm -f "$ZIP"
run sh -c "cd '$STAGE' && zip -qrX '$ZIP' Tallylight"

say "paquet Swift dans $PACKAGE_DIR"
run mkdir -p "$PACKAGE_DIR/Sources/TallylightKit"
run rm -rf "$PACKAGE_DIR/$FRAMEWORK"
run mv "$PACKAGE_STAGE/$FRAMEWORK" "$PACKAGE_DIR/$FRAMEWORK"
run cp "$PACKAGE_STAGE/Sources/TallylightKit/Tallylight.swift" "$PACKAGE_DIR/Sources/TallylightKit/"
if [ "$PACKAGE_DIR" != "$ROOT/apple" ]; then
    run cp "$ROOT/apple/Package.swift" "$PACKAGE_DIR/Package.swift"
fi
# Les bibliothèques intermédiaires pèsent lourd et ne servent plus
run rm -rf "$STAGE/lib"

if [ -z "$DRY_RUN" ]; then
    # Contrôle : chaque tranche a sa bibliothèque, son en-tête et sa carte de module
    plutil -lint "$PACKAGE_DIR/$FRAMEWORK/Info.plist" >/dev/null || fail "Info.plist du xcframework illisible"
    while IFS='|' read -r dir _ _ archs; do
        [ -n "$dir" ] || continue
        slice="$PACKAGE_DIR/$FRAMEWORK/$dir"
        for file in "$LIB" Headers/TallylightFFI/TallylightFFI.h Headers/TallylightFFI/module.modulemap; do
            [ -f "$slice/$file" ] || fail "$dir/$file manque dans le xcframework"
        done
        for arch in $archs; do
            lipo "$slice/$LIB" -verify_arch "$arch" || fail "$dir : architecture $arch absente"
        done
    done <<EOF
$SLICES
EOF
    grep -q 'import TallylightFFI' "$PACKAGE_DIR/Sources/TallylightKit/Tallylight.swift" \
        || fail "Tallylight.swift n'importe pas TallylightFFI"
    say "prêt : $PACKAGE_DIR ($FRAMEWORK, $(du -sh "$PACKAGE_DIR/$FRAMEWORK" | cut -f1)) et $ZIP ($(du -h "$ZIP" | cut -f1))"
fi
