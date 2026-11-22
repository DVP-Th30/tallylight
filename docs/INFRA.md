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
tallylight install      # ajoute les hooks aux réglages de Claude Code (sauvegarde à côté), à ceux de Codex s'il est là, et lance le service
tallylight uninstall    # retire les hooks et le service, garde les données
```
Codex : hooks dans `~/.codex/hooks.json` (ou `$CODEX_HOME`), sauvegarde dans `hooks.json.before-tallylight`, seulement si le dossier existe ou si `codex` est dans le `PATH`. Codex demande à l'utilisateur d'approuver les nouveaux hooks à son lancement suivant (`/hooks`) ; sans cela ils ne tournent pas.

Le service est une unité utilisateur systemd sous Linux, un agent de lancement sous macOS (`~/Library/LaunchAgents/dev.tallylight.daemon.plist`, journal dans `~/Library/Logs/tallylight.log`). Les hooks des autres outils (Vibe Island…) ne sont pas touchés.

`install` attend que le démon réponde sur son port local (20 s au plus) avant d'annoncer « démarré » : le lanceur rend la main avant que le démon écoute. Sans réponse, il le dit et renvoie au journal.

## Intégration continue
- `ci.yml` : format, clippy et tests à chaque poussée sur `main`.
- `release.yml` : publie une version sur étiquette `v*` (voir « Publier une version »).
- `pages.yml` : publie `site/` sur GitHub Pages, seulement quand le dépôt est public.
- `binaries.yml` : lancé à la main (`gh workflow run binaries`), construit `tallylight` pour macOS (Apple Silicon) et Linux et les dépose dans la version de développement `dev`. Pas automatique : une machine macOS compte dix fois dans le quota d'Actions.
```sh
gh release download dev -R DVP-Th30/tallylight -p 'tallylight-aarch64-apple-darwin' -O ~/.local/bin/tallylight --clobber   # sur le Mac
```

## Publier une version
1. Mettre la version dans `Cargo.toml` (`[workspace.package] version`, et les dépendances internes des crates), compléter `CHANGELOG.md`, committer.
2. Répétition : `gh workflow run release` construit les quatre binaires sans rien publier (Linux x86_64 et arm64 en musl, donc statiques ; macOS Apple Silicon et Intel).
3. `git tag v0.1.0 && git push origin v0.1.0` : `release.yml` crée la version GitHub avec, par plateforme, `tallylight-<cible>.tar.gz` et son empreinte `.sha256`.
4. `site/install.sh` (servi par GitHub Pages) télécharge la dernière version, vérifie l'empreinte et installe dans `~/.local/bin`. Il ne fonctionne qu'une fois le dépôt public et une version `v*` publiée.

`binaries.yml` (version `dev`) reste pour essayer la branche `main` sans publier.

## Passage en public (liste à dérouler le jour J, décision D7)
Fait le 2026-11-22 : dépôt public, Pages et signalement privé de failles activés, version `v0.1.0` publiée, sujets et page d'accueil renseignés, `install-test` vert sur les dix systèmes avec la commande du site (run 45048191849). Reste l'étape 7, facultative. La liste est gardée pour mémoire.
1. Vérifier que la CI est verte sur `main` et que `gh workflow run release` passe.
2. Rendre le dépôt public (réglages GitHub, ou `gh repo edit DVP-Th30/tallylight --visibility public --accept-visibility-change-consequences`).
3. Activer Pages en mode Actions : `gh api -X POST repos/DVP-Th30/tallylight/pages -f build_type=workflow`, puis `gh workflow run pages`. Adresse : `https://dvp-th30.github.io/tallylight/`.
4. Activer le signalement privé de failles (Settings → Security → Private vulnerability reporting) : `SECURITY.md` y renvoie.
5. Publier la première version (étape 3 ci-dessus), puis essayer `curl -fsSL https://dvp-th30.github.io/tallylight/install.sh | sh` sur une machine vierge.
6. Renseigner la page du dépôt : `gh repo edit --homepage https://dvp-th30.github.io/tallylight/ --add-topic claude-code --add-topic coding-agents --add-topic rust --add-topic iroh --add-topic a2a`.
7. Facultatif : réserver le nom sur crates.io (`cargo publish`, irréversible) ; les crates `tallylight-proto` et `tallylight-bindgen` ne se publient pas (`publish = false`).

Essai sur machines vierges : `gh workflow run install-test` installe la dernière version par la commande du site sur dix systèmes neufs (Ubuntu Intel et ARM, macOS Apple Silicon et Intel, conteneurs Debian 12, Ubuntu 20.04, Alpine Intel et ARM, Fedora, Arch) puis déroule `.github/scripts/smoke.sh` : hooks, démon, un événement, appairage (et, à partir de la version qui connaît Codex, ses hooks, un événement relayé par `tallylight hook codex` et le retrait). Avec `-f source=release-assets`, il prend les fichiers de la version au lieu du site (seul moyen tant que le dépôt est privé).

Historique vérifié le 2026-11-22 : aucun secret ni adresse IP ; seuls restent des noms de machines et de dossiers personnels dans les docs.

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

Paquet prêt à intégrer (2026-11-20) : `android/build-aar.sh` produit `target/android/tallylight-<version>.aar`, qui contient la bibliothèque native (profil `embed`, segments alignés sur 16 Ko), les liaisons Kotlin compilées, `dev.tallylight.Android`, les permissions (dont `ACCESS_LOCAL_NETWORK`) et les règles R8. L'application n'ajoute que JNA (`net.java.dev.jna:jna:5.x@aar`). `--dry-run` affiche les étapes, `--stub-native` vérifie la chaîne sans NDK. Variables utiles : `ANDROID_HOME`, `KOTLINC`, `ABIS` (défaut `arm64-v8a x86_64`). Sur serveurdvp :
```sh
ANDROID_HOME=~/android-sdk KOTLINC=~/opt/kotlinc-2.4.21/bin/kotlinc ABIS=arm64-v8a memlimit 2.8 android/build-aar.sh
```
Vérifié le 2026-11-20 sur serveurdvp (NDK 29, kotlinc 2.4.21 installé dans `~/opt`) : AAR de 3,5 Mo, bibliothèque arm64 de 7,4 Mo sans dépendance hors du système, environ 9 minutes. Pas encore intégré dans Encre. La cible Rust `x86_64-linux-android` (émulateur) n'est pas installée sur serveurdvp.

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
- Hooks : ajoutés à `~/.claude/settings.json`, sauvegarde dans `~/.claude/settings.json.before-tallylight`.
- Données : `~/.local/share/tallylight/`, comme sous Linux.
- Mettre à jour : remplacer le binaire puis `tallylight install` (recharge l'agent).
- Premier essai : `tallylight pair` lancé dans la seconde qui suivait `install` a répondu « Connection refused ». Le démon n'avait pas planté : il attendait d'avoir joint le réseau pour ouvrir son port. Corrigé des deux côtés : le port s'ouvre dès le lancement (DECISIONS D10) et `install` attend qu'il réponde.
- Écrans appairés : le terminal du Mac et Encre (Galaxy A36), en lien direct sur le Wi-Fi.

## Appareils de test
- serveurdvp (Linux) et le Mac : cœur.
- Galaxy A36 (Android) : écran de démo, via le launcher Encre (`~/projets/DVPLauncher`, accès adb décrit dans son `docs/INFRA.md`).

## Secrets
Aucun pour l'instant. Rien dans le dépôt : `.gitignore` exclut `secrets.*` et `.env`.
