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

Le service est une unité utilisateur systemd sous Linux, un agent de lancement sous macOS (`~/Library/LaunchAgents/dev.tallylight.daemon.plist`, journal dans `~/Library/Logs/tallylight.log`), une tâche planifiée sous Windows (voir « Windows » plus bas). Les hooks des autres outils (Vibe Island…) ne sont pas touchés.

`install` attend que le démon réponde sur son port local (20 s au plus) avant d'annoncer « démarré » : le lanceur rend la main avant que le démon écoute. Sans réponse, il le dit et renvoie au journal.

## Intégration continue
- `ci.yml` : format, clippy et tests à chaque poussée sur `main` et à chaque demande de fusion. Un second travail, sur `windows-latest`, passe clippy et les tests des trois crates attendues sous Windows (`tallylight`, `tallylight-core`, `tallylight-client`), puis déroule `.github/scripts/smoke.ps1` (hooks, démon, un événement, appairage, dossiers par défaut de Windows) et `.github/scripts/service.ps1` (tâche planifiée : installation, relance, relais gardé, désinstallation).
- `release.yml` : publie une version sur étiquette `v*` (voir « Publier une version »).
- `swift.yml` : construit le paquet Swift (iOS, simulateur, macOS) et l'essaie, quand il est concerné (voir « Compiler pour iOS et macOS »).
- `pages.yml` : publie `site/` sur GitHub Pages, seulement quand le dépôt est public.
- `binaries.yml` : lancé à la main (`gh workflow run binaries`), construit `tallylight` pour macOS (Apple Silicon) et Linux et les dépose dans la version de développement `dev`. Pas automatique : une machine macOS compte dix fois dans le quota d'Actions.
```sh
gh release download dev -R DVP-Th30/tallylight -p 'tallylight-aarch64-apple-darwin' -O ~/.local/bin/tallylight --clobber   # sur le Mac
```

## Publier une version
1. Mettre la version dans `Cargo.toml` (`[workspace.package] version`, et les dépendances internes des crates), compléter `CHANGELOG.md`, committer.
2. Répétition : `gh workflow run release` construit les cinq binaires sans rien publier (Linux x86_64 et arm64 en musl, donc statiques ; macOS Apple Silicon et Intel ; Windows x86_64). Sous Windows, la répétition installe aussi l'archive obtenue par `site/install.ps1` (servie localement, sous Windows PowerShell 5.1), déroule les deux scripts d'essai et rejoue l'installation par-dessus un service en marche.
3. `git tag v0.1.0 && git push origin v0.1.0` : `release.yml` crée la version GitHub avec, par plateforme, `tallylight-<cible>.tar.gz` (`.zip` sous Windows) et son empreinte `.sha256`. Le travail `swift` y joint `tallylight-swift-<version>.zip` (le paquet Swift) et son empreinte.
4. `site/install.sh` (servi par GitHub Pages) télécharge la dernière version, vérifie l'empreinte et installe dans `~/.local/bin`. Il ne fonctionne qu'une fois le dépôt public et une version `v*` publiée. Sous Windows, `site/install.ps1` fait de même vers `%LOCALAPPDATA%\tallylight\bin` (`irm https://dvp-th30.github.io/tallylight/install.ps1 | iex`) ; il ne trouvera une archive qu'à partir de la première version publiée après la 0.1.0.

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

Version `v0.1.1` publiée le 2026-11-23 (Windows, Codex, paquet Swift ; AAR reconstruit avec iroh 1.4.2) : `install-test` vert sur les dix systèmes et sur Windows avec la commande publique (run 45081817913).

Essai sur machines vierges : `gh workflow run install-test` installe la dernière version par la commande du site sur dix systèmes neufs (Ubuntu Intel et ARM, macOS Apple Silicon et Intel, conteneurs Debian 12, Ubuntu 20.04, Alpine Intel et ARM, Fedora, Arch) puis déroule `.github/scripts/smoke.sh` : hooks, démon, un événement, appairage (et, à partir de la version qui connaît Codex, ses hooks, un événement relayé par `tallylight hook codex` et le retrait). Avec `-f source=release-assets`, il prend les fichiers de la version au lieu du site (seul moyen tant que le dépôt est privé).

Bibliothèque Android : le workflow `android` construit l'AAR (arm64 et x86_64) à chaque étiquette `v*` et le joint à la version. `gh workflow run android -f tag=v0.1.0` complète une version existante ; sans `tag`, c'est une répétition.

Historique vérifié le 2026-11-22 : aucun secret ni adresse IP ; seuls restent des noms de machines et de dossiers personnels dans les docs.

## Windows (2026-11-22, décision D16)
- Binaire : `%LOCALAPPDATA%\tallylight\bin\tallylight.exe` (ajouté au `PATH` de l'utilisateur par `install.ps1`).
- Tâche planifiée `tallylight` : `schtasks /Query /TN tallylight /V /FO LIST` pour son état, `schtasks /Run /TN tallylight` et `schtasks /End /TN tallylight` pour la lancer et l'arrêter. Journal du démon : `%LOCALAPPDATA%\tallylight\daemon.log`.
- Hooks : `%USERPROFILE%\.claude\settings.json`, sauvegarde dans `settings.json.before-tallylight`.
- Données : `%LOCALAPPDATA%\tallylight\` (`key`, `ticket`, `devices.json`, `sessions.json`, `service.xml`).
- Mettre à jour : relancer `install.ps1` (il arrête la tâche, remplace le binaire, la relance), ou arrêter la tâche avant de remplacer le binaire à la main.
- Pas de machine Windows parmi les appareils de test : tout a été vérifié sur les machines `windows-latest` de GitHub Actions le 2026-11-22 — `ci.yml` (run 45060885057 : clippy, tests, démon, appairage par un relais, tâche planifiée lancée, relancée et retirée, journal écrit) et la répétition `gh workflow run release --ref feat/windows` (run 45060883944 : archive zip, installation par `install.ps1`, mise à jour par-dessus un service en marche). `install-test.yml` a un cas Windows depuis la 0.1.1 (commande du site, puis les deux scripts d'essai), en mode `public-url` seulement.

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

## Compiler pour iOS et macOS (Swift)
`apple/build-xcframework.sh` (2026-11-22, décision D18) remplit le paquet Swift `apple/` : `TallylightFFI.xcframework` (bibliothèque statique `libtallylight_ffi.a`, profil `embed`, une tranche pour iPhone, une pour le simulateur, une pour macOS Apple Silicon et Intel réunis par `lipo`, chacune avec l'en-tête C) et `Sources/TallylightKit/Tallylight.swift` (liaisons générées par `tallylight-bindgen --language swift`, noms réglés dans `crates/tallylight-ffi/uniffi.toml`). Il produit aussi `target/apple/tallylight-swift-<version>.zip`, le paquet entier prêt à décompresser. Ni le xcframework ni le fichier Swift ne sont dans le dépôt.
```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin x86_64-apple-darwin
apple/build-xcframework.sh
apple/smoke-test.sh
```
- `--dry-run` affiche les étapes. Variables : `IOS_TARGETS`, `IOS_SIM_TARGETS`, `MACOS_TARGETS` (une liste vide retire la plateforme ; ajouter `x86_64-apple-ios` à `IOS_SIM_TARGETS` pour les simulateurs des Mac Intel), `IOS_MIN` (15.0), `MACOS_MIN` (12.0).
- Les cibles iOS demandent Xcode complet : la bibliothèque de chiffrement (`ring`) compile du C contre le SDK de l'iPhone. Avec les seuls outils en ligne de commande, on ne construit que macOS (`IOS_TARGETS= IOS_SIM_TARGETS= apple/build-xcframework.sh`) et le script écrit lui-même le dossier du xcframework, faute de `xcodebuild -create-xcframework`.
- `apple/smoke-test.sh` lance un démon d'essai (port 47796, dossier personnel temporaire), lui envoie un événement, appaire le programme `apple/smoke` (Swift, construit par SwiftPM sur le paquet) et attend qu'il reçoive la session. `ios-link` lie le même programme pour iPhone sans le lancer ; `ios-simulator` le fait tourner dans un simulateur.
- CI : `swift.yml` (machine macOS, Xcode complet) déroule le script entier et les trois essais quand `apple/`, les crates de la bibliothèque ou `Cargo.lock` changent ; `release.yml` joint l'archive du paquet à chaque version.

Côté application :
- Ajouter le dossier `apple/` (ou l'archive décompressée) comme paquet local, produit `TallylightKit`, puis `import TallylightKit`. Le paquet lie `Security`, `SystemConfiguration` et `CoreFoundation`, plus `Network` et `libresolv` sur iOS (liste donnée par `rustc --print native-static-libs`, que le script affiche à chaque compilation).
- Aucune initialisation propre à iOS : l'équivalent de `dev.tallylight.Android.init` n'existe pas (la doc d'iroh ne décrit ce besoin que pour Android).
- **Réseau local sur iOS** : déclarer `NSLocalNetworkUsageDescription` dans `Info.plist`. Apple (note technique TN3179, révision du 2026-10-06) range l'envoi d'UDP vers une adresse du réseau local parmi les opérations soumises à l'autorisation « Réseau local » depuis iOS 14, quelle que soit l'API, et le système bloque l'opération tant que l'utilisateur n'a pas accepté. La clé donne le texte de la demande ; Apple écrit que toute application qui utilise le réseau local « should include » cette description. Ni `NSBonjourServices` (réservé à Bonjour) ni le droit `com.apple.developer.networking.multicast` (multidiffusion) ne sont nécessaires : la bibliothèque est compilée sans les options par défaut d'iroh, donc sans ouverture de ports sur la box (UPnP) ni recherche mDNS.
- Ce point n'est **pas vérifié sur un iPhone**, et les sources se contredisent : l'application d'exemple de l'éditeur d'iroh (`n0-computer/hello-iroh-ffi`, README Swift) omet la clé en affirmant que le trafic QUIC direct ne déclenche pas la demande, alors que le ticket iroh #3474 (ouvert) rapporte la demande sans mDNS. Attendu en cas de refus, par analogie avec Android 17 (constaté, voir plus haut) : pas de lien direct sur le même Wi-Fi, tout passe par un relais. Le simulateur n'applique pas cette règle (TN3179) : seul un essai sur appareil tranchera.
- macOS : la même règle existe depuis macOS 15 pour les applications ; un outil lancé depuis le Terminal en est dispensé (TN3179), ce qui est le cas de l'essai. Une application en bac à sable doit en plus demander `com.apple.security.network.client` et `com.apple.security.network.server` (doc d'iroh).
- Non vérifié non plus : le manifeste de confidentialité exigé par l'App Store (API à « raison requise », dont les horodatages de fichiers et l'horloge système qu'utilise la bibliothèque standard de Rust ; ticket rust-lang/rust #114186, sans consigne officielle).

Vérifié le 2026-11-22 sur un Mac Apple Silicon sans Xcode (outils en ligne de commande, Swift 6.3, Rust 1.99) : le script construit la tranche macOS (Apple Silicon et Intel, bibliothèque de 30 Mo pour les deux, archive du paquet de 9 Mo), `swift build` résout le xcframework, et `apple/smoke-test.sh` reçoit en Swift la session d'un démon d'essai, en lien direct. Les tranches iOS ne se compilent pas sur cette machine (pas de SDK).

Vérifié le même jour en CI (`swift.yml`, run 45063515705, macOS 26 et Xcode 26.6) : les quatre compilations (environ 2 min 30 chacune), `xcodebuild -create-xcframework` avec les trois tranches `ios-arm64`, `ios-arm64-simulator` et `macos-arm64_x86_64` (bibliothèque de 15,6 Mo par tranche iOS, xcframework de 60 Mo, archive de 18 Mo), l'essai macOS sur ce xcframework, l'édition de liens du programme d'essai pour iPhone (exécutable de 7,6 Mo, ce que pèse la bibliothèque une fois liée) et le même essai de bout en bout dans un simulateur d'iPhone 17 Pro, en lien direct. Deux défauts trouvés et corrigés par ces essais : iroh 1.4.1 ne compilait pas pour iOS (décision D16) et il manquait `libresolv` à l'édition de liens.

Pas vérifié : une vraie application iOS (le programme d'essai est une commande lancée par `simctl spawn`, sans `Info.plist` ni bac à sable), un iPhone réel, l'autorisation « Réseau local », l'ajout du paquet dans un projet Xcode (seuls `swift build` sur macOS et `swiftc` pour iOS ont servi), la tranche Intel du simulateur.

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
