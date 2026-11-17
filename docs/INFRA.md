# Infra

## Dépôt
`DVP-Th30/tallylight`, privé (voir DECISIONS D7). Clone de travail : `~/projets/tallylight` sur serveurdvp.

## Compiler et vérifier
Rust stable (1.100 sur serveurdvp, binaires dans `~/.cargo/bin`).
```sh
cargo test
cargo clippy -- -D warnings
cargo fmt --check
```
Sur serveurdvp la mémoire est comptée : lancer les grosses compilations sous `memlimit` (voir la configuration globale).

## Compiler pour Android
NDK 29 dans `~/android-sdk/ndk/29.0.14206865`, cible Rust `aarch64-linux-android` (installés le 2026-11-17).
```sh
TC=~/android-sdk/ndk/29.0.14206865/toolchains/llvm/prebuilt/linux-x86_64/bin
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=$TC/aarch64-linux-android34-clang
export CC_aarch64_linux_android=$TC/aarch64-linux-android34-clang AR_aarch64_linux_android=$TC/llvm-ar
memlimit 3.5 cargo build --release -p tallylight-proto --target aarch64-linux-android   # environ 8 min la première fois
```
Bibliothèque pour une application Android, et ses liaisons Kotlin :
```sh
cargo build --profile embed -p tallylight-ffi --lib --target aarch64-linux-android   # libtallylight_ffi.so
cargo build -p tallylight-ffi                                                        # générateur et bibliothèque hôte
target/debug/uniffi-bindgen generate --library target/debug/libtallylight_ffi.so --language kotlin --no-format --out-dir <dossier kotlin>
```
Côté application : dépendance JNA (`net.java.dev.jna:jna`, variante aar), règles R8 qui gardent `com.sun.jna`, `uniffi` et `dev.tallylight`, et un objet Kotlin `dev.tallylight.Android` avec `external fun init(context)`. Encre fait tout cela par `tools/tallylight.sh`.

Lancer sur le téléphone sans application, par adb :
```sh
adb push target/aarch64-linux-android/release/tallylight-proto /data/local/tmp/
adb shell /data/local/tmp/tallylight-proto connect <ticket>
```

## Installation sur serveurdvp (2026-11-17)
- Binaire : `~/.local/bin/tallylight` (copie de `target/release/tallylight`).
- Service utilisateur systemd `tallylight` (`~/.config/systemd/user/tallylight.service`, plafond mémoire 200 Mo, environ 7 Mo utilisés) : `systemctl --user status tallylight`, journal par `journalctl --user -u tallylight`.
- Hooks : ajoutés à `~/.claude/settings.json` à côté de ceux de Vibe Island, et dans le modèle `claude-config/settings/server.json`. Sauvegarde de l'ancien fichier dans `~/.claude/backups/`. Claude Code les prend en compte sans redémarrer les sessions. Le Mac n'est pas concerné.
- Données : `~/.local/share/tallylight/` (`key`, `ticket`, lisibles par le seul propriétaire).
- Mettre à jour : recompiler, recopier le binaire, `systemctl --user restart tallylight`.
- Désinstaller : `systemctl --user disable --now tallylight`, puis retirer des réglages les hooks dont l'URL contient `47700/hooks/claude`.

## Appareils de test
- serveurdvp (Linux) et le Mac : cœur.
- Galaxy A36 (Android) : écran de démo, via le launcher Encre (`~/projets/DVPLauncher`, accès adb décrit dans son `docs/INFRA.md`).

## Secrets
Aucun pour l'instant. Rien dans le dépôt : `.gitignore` exclut `secrets.*` et `.env`.
