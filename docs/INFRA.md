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
Lancer sur le téléphone sans application, par adb :
```sh
adb push target/aarch64-linux-android/release/tallylight-proto /data/local/tmp/
adb shell /data/local/tmp/tallylight-proto connect <ticket>
```

## Appareils de test
- serveurdvp (Linux) et le Mac : cœur.
- Galaxy A36 (Android) : écran de démo, via le launcher Encre (`~/projets/DVPLauncher`, accès adb décrit dans son `docs/INFRA.md`).

## Secrets
Aucun pour l'instant. Rien dans le dépôt : `.gitignore` exclut `secrets.*` et `.env`.
