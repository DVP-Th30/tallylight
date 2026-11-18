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

## Installer sur une machine
```sh
tallylight install      # ajoute les hooks aux réglages de Claude Code (sauvegarde à côté) et lance le service
tallylight uninstall    # retire les hooks et le service, garde les données
```
Le service est une unité utilisateur systemd sous Linux, un agent de lancement sous macOS (`~/Library/LaunchAgents/dev.tallylight.daemon.plist`, journal dans `~/Library/Logs/tallylight.log`). Les hooks des autres outils (Vibe Island…) ne sont pas touchés.

`install` attend que le démon réponde sur son port local (20 s au plus) avant d'annoncer « démarré » : le démon n'ouvre ce port qu'une fois joint au réseau, soit quelques secondes après son lancement. Sans réponse, il le dit et renvoie au journal.

## Intégration continue
- `ci.yml` : format, clippy et tests à chaque poussée sur `main`.
- `binaries.yml` : lancé à la main (`gh workflow run binaries`), construit `tallylight` pour macOS (Apple Silicon) et Linux et les dépose dans la version de développement `dev`. Pas automatique : une machine macOS compte dix fois dans le quota d'Actions.
```sh
gh release download dev -R DVP-Th30/tallylight -p 'tallylight-aarch64-apple-darwin' -O ~/.local/bin/tallylight --clobber   # sur le Mac
```

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
cargo build -p tallylight-ffi -p tallylight-bindgen                                  # bibliothèque hôte et générateur
target/debug/tallylight-bindgen generate --library target/debug/libtallylight_ffi.so --language kotlin --no-format --out-dir <dossier kotlin>
```
Côté application : dépendance JNA (`net.java.dev.jna:jna`, variante aar), règles R8 qui gardent `com.sun.jna`, `uniffi` et `dev.tallylight`, et un objet Kotlin `dev.tallylight.Android` avec `external fun init(context)`. Encre fait tout cela par `tools/tallylight.sh`.

Android 17 (application ciblant le SDK 37) : sans la permission `android.permission.ACCESS_LOCAL_NETWORK`, le système refuse à l'application toute connexion vers le réseau local. La connexion directe à une machine du même Wi-Fi échoue alors en silence et tout passe par un relais (constaté le 2026-11-18 avec Encre : `RELAYED` sans la permission, `DIRECT` avec). Un binaire lancé par adb n'est pas soumis à cette règle, ce qui masquait le problème lors du premier essai.

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

## Installation sur le Mac (2026-11-18)
- Binaire : `~/.local/bin/tallylight`, pris dans la version `dev` ou compilé sur place (`cargo build --release -p tallylight`, Rust 1.99 dans `~/.cargo/bin`).
- Agent de lancement `dev.tallylight.daemon` : `launchctl print gui/$(id -u)/dev.tallylight.daemon`, journal dans `~/Library/Logs/tallylight.log`.
- Hooks : ajoutés à `~/.claude/settings.json`, sauvegarde dans `~/.claude/settings.json.avant-tallylight`.
- Données : `~/.local/share/tallylight/`, comme sous Linux.
- Mettre à jour : remplacer le binaire puis `tallylight install` (recharge l'agent).
- Premier essai : `tallylight pair` lancé dans la seconde qui suivait `install` a répondu « Connection refused ». Le démon n'avait pas planté, il n'avait pas encore ouvert son port ; d'où l'attente ajoutée à `install`.
- Écrans appairés : le terminal du Mac et Encre (Galaxy A36), en lien direct sur le Wi-Fi.

## Appareils de test
- serveurdvp (Linux) et le Mac : cœur.
- Galaxy A36 (Android) : écran de démo, via le launcher Encre (`~/projets/DVPLauncher`, accès adb décrit dans son `docs/INFRA.md`).

## Secrets
Aucun pour l'instant. Rien dans le dépôt : `.gitignore` exclut `secrets.*` et `.env`.
