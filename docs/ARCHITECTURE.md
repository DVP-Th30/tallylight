# Architecture

État au 2026-11-17 : première chaîne complète sur une machine — une vraie session Claude Code, ses hooks, le démon, un écran terminal relié par iroh.

## Promesse
Le lien clé en main entre n'importe quel agent de code et n'importe quel écran : un binaire, un format d'état standard, un réseau privé intégré, des clients pour chaque plateforme.

## Composants visés
- **Cœur (`tallylightd`)** — un binaire par machine. Installe les hooks des agents présents, reçoit leurs événements, tient l'état des sessions et le diffuse aux appareils appairés.
- **Bibliothèque cliente** — le même code Rust compilé pour Linux, macOS, Windows, Android, iOS et le navigateur. Une application l'intègre pour recevoir « quelles sessions, dans quel état ».
- **Ponts simples** — sortie HTTP locale et sortie Bluetooth compatible avec le protocole d'objets d'Anthropic (`claude-desktop-buddy`), pour un écran ESP32 qui n'embarque pas le réseau.
- **Réveil (optionnel, plus tard)** — petit service à déployer par l'éditeur d'une application, avec sa propre clé FCM/APNs, pour notifier un téléphone en veille.

## En code aujourd'hui
- `crates/tallylight-core` — modèle partagé : `State` (vocabulaire A2A), `Session`, `Snapshot`, et une traduction par agent d'un hook en changement d'état (fonctions pures, testées) : `claude::change` pour Claude Code, `codex::change` pour Codex, qui reprend le même corps d'événement (`claude::HookEvent`).
- `crates/tallylight` — le binaire :
  - `tallylight daemon [--port N] [--relay R]` : reçoit les hooks sur `http://127.0.0.1:47700/hooks/claude` (Claude Code) et `/hooks/codex` (Codex), tient les sessions (`store.rs`), sert `GET /sessions`, et pousse (le port local est ouvert dès le lancement, le réseau est joint ensuite en tâche de fond, décision D10) un instantané JSON par ligne à chaque écran appairé connecté par iroh (ALPN `tallylight/state/1`). Données dans `~/.local/share/tallylight/` (`%LOCALAPPDATA%\tallylight` sous Windows) : `key` (identité, donc ticket stable), `ticket`, `devices.json` (écrans autorisés).
  - `tallylight watch [--relay R] [appairage]` : écran terminal ; suit cette machine si son démon répond (il fournit lui-même l'appairage), toutes les machines déjà appairées depuis ce terminal, et celle de la chaîne donnée en argument (vérifiée tout de suite : un code faux ou périmé est signalé). Une tâche par machine, reconnexion après coupure (attente de 2 à 60 s, tentative bornée à 10 s) ; chaque machine affiche son lien (« relais », « hors ligne », « non appairé »). Les sessions qui attendent l'utilisateur passent en tête. L'écran garde sa propre identité (`screen-key`) et la liste des machines appairées (`machines`).
  - `tallylight pair` : affiche un QR et une chaîne d'appairage à usage unique ; `tallylight devices` et `tallylight unpair <identité>` gèrent les écrans autorisés (`devices.rs`, décision D8). Ces commandes parlent au démon par son port local (`local.rs`).
  - `tallylight install [--no-service] [--relay R]` / `uninstall` (`install.rs`) : ajoute ou retire les hooks dans les réglages de Claude Code (sauvegarde, ordre des clés conservé, hooks des autres outils intacts), fait de même dans le `hooks.json` de Codex quand Codex est sur la machine (dossier `~/.codex` présent ou `codex` dans le `PATH`), et installe le service (systemd utilisateur sous Linux, agent de lancement sous macOS, tâche planifiée à l'ouverture de session sous Windows, décision D16). `tallylight hooks [claude|codex]` affiche les blocs pour qui préfère les coller à la main.
  - `tallylight hook codex [--port N]` (`local.rs`) : la commande que Codex lance à chaque événement, faute de hook `http`. Elle lit l'événement sur son entrée, en retire la sortie de l'outil et le poste au démon ; elle ne dit rien et sort toujours en succès, démon arrêté compris (décision D17).
  - `tallylight name [nom | --reset]` : nom de la machine sur les écrans (« Mac » plutôt que le nom système « Th30 »), gardé dans le fichier `name` du dossier de données et repris aussitôt par le démon (`POST /name/reload`), sans redémarrage.
  - Relais iroh (`--relay R`, sinon `TALLYLIGHT_RELAY`, décision D11) : `n0` (relais publics de l'éditeur d'iroh, le défaut) ou une ou plusieurs adresses `https://…` séparées par des virgules, schéma, hôte et port seulement. Le démon annonce le relais joint dans son ticket ; `install` inscrit le choix dans le service et le conserve lors d'une réinstallation sans `--relay`.
- `crates/tallylight-client` — la bibliothèque d'écran : `Screen::new(clé, nom)`, `screen.watch(appairage)` puis `watch.next()` pour chaque instantané. `tallylight watch` repose dessus. Le protocole partagé (`Hello`, `Welcome`, ALPN) vit dans `tallylight-core::protocol`.
- `crates/tallylight-ffi` — liaisons pour les applications, générées par UniFFI (Kotlin, et Swift depuis le 2026-11-22) : objet `Tallylight(data_dir, name)` (ou `Tallylight.withRelay(data_dir, name, relay)` pour ses propres relais) avec `start(listener)`, `pair(pairing)`, `forget(id)`, `stop()`, `machines()`. Un écran suit plusieurs machines : l'application reçoit `on_machine` à chaque changement d'une machine (identité stable, nom, lien direct / relais / non appairé / hors ligne, sessions) et `on_machine_removed`. La bibliothèque garde l'identité de l'écran (`screen-key`) et les machines appairées (`machines.json`), partage un seul point réseau entre elles et se reconnecte seule (attente de 2 à 60 s). Elle garde en mémoire les dernières sessions reçues de chaque machine d'un `stop()` à l'autre : au `start()` suivant (rallumage de l'écran), chaque machine s'annonce aussitôt « connexion » avec ces sessions, au lieu d'une liste vide le temps de se reconnecter, ou indéfiniment si la machine ne répond plus. Sur Android, `dev.tallylight.Android.init(context)` fournit le contexte dont iroh a besoin pour lire les serveurs DNS. Rien de tel sur iOS ni macOS : le code propre à Android (`jni`, initialisation) n'est compilé que pour cette cible. Côté Apple, `apple/` est un paquet Swift (module `TallylightKit`, bibliothèque statique dans `TallylightFFI.xcframework`) rempli par `apple/build-xcframework.sh` (décision D16). Le générateur de liaisons est dans `crates/tallylight-bindgen`.
- Langue : tout ce que le binaire affiche ou journalise est en anglais depuis le 2026-11-22 (décision D14) ; `tallylight --help` décrit les commandes, `tallylight --version` donne la version.
- Windows (x86_64, décision D16) : le même binaire. Ce qui diffère est regroupé dans `windows.rs` (existence d'un processus, démon détaché de sa console, séquences d'écran de `watch`) et dans les parties `cfg(windows)` de `install.rs` (tâche planifiée) et de `main.rs` (dossiers : `%USERPROFILE%\.claude`, `%LOCALAPPDATA%\tallylight`). Vérifié sur les machines Windows de GitHub Actions seulement, sans vraie session Claude Code.
- `crates/tallylight-proto` — prototype réseau jetable du premier jalon.
- `site/index.html` — page de présentation (anglais, fichier autonome sans dépendance hormis Google Fonts), destinée à GitHub Pages quand le dépôt sera public (décision D7). Les affirmations de la page (latences, comparaison avec l'existant) viennent de ce document et de `ETAT-DE-L-ART.md` : les revérifier avant toute publication.

Chaque session porte aussi `tool` et `detail` : ce que fait l'agent, ou ce pour quoi il attend une autorisation, en une ligne de 140 caractères au plus tirée des arguments de l'outil (description d'une commande, nom d'un fichier, motif cherché, question posée). S'y ajoutent `last_message` (les 600 premiers caractères du dernier message de l'agent, à la fin d'un tour, effacés au tour suivant) et `url` (l'adresse de la session dans l'application Claude, pour les sessions suivies par Remote Control ; tirée de la fiche de session). Enfin `model` : l'identifiant du dernier modèle qui a répondu (« claude-opus-5-5 »), lu en fin de transcription (`transcript.rs`, décision D13) ; la bibliothèque d'écran le donne en clair (« Opus 5.5 », `model_label`). Par précaution, une commande sans description n'est résumée que par le nom du programme, et rien du contenu des fichiers ni de la sortie des commandes ne sort de la machine.

Correspondance Claude Code → état : `UserPromptSubmit`, `PreToolUse`, `PostToolUse` → `working` ; `PermissionRequest` et `Notification` (`permission_prompt`, `elicitation_*`) → `input-required` ; `Stop` → `completed` ; `StopFailure` → `failed` ; `SessionEnd` → session retirée. Les `Stop` de sous-agents sont ignorés.

Correspondance Codex → état (`agent` vaut `codex`) : `UserPromptSubmit`, `PreToolUse`, `PostToolUse` → `working` ; `PermissionRequest` → `input-required` ; `Stop` → `completed` ; `Interrupt` → `canceled` ; `SessionEnd` → session retirée ; `SubagentStop` et les autres événements sont ignorés. Jamais `failed` : Codex n'a pas d'événement pour un tour qui échoue. Le modèle vient de l'événement (`model`), pas de la transcription. Pas de titre, pas d'adresse, et pas de fiche de session : le rapprochement de D9 ne s'applique pas, une session Codex n'existe que par ses hooks (détails et limites en D17).

Vérifié le 2026-11-22 sous macOS avec le vrai Codex 0.171.0 et un faux modèle local (aucun compte OpenAI) : hooks posés par `tallylight install`, puis au travail (Bash, « echo ») → fini avec le dernier message ; demande d'autorisation dans l'interface du terminal → « attend une réponse » avec le motif de la demande ; interruption → `Interrupt` reçu ; fin de `codex exec` → session retirée. Pas encore vu avec un vrai modèle d'OpenAI.

Vérifié sous macOS le 2026-11-18 : agent de lancement, quatre sessions ouvertes vues par `tallylight watch`, Encre appairé en lien direct.

Vérifié le 2026-11-17 avec une vraie session (`claude -p --settings <fichier de hooks>`, outil Read) : l'écran a affiché au travail → au travail (Read) → fini, puis la session a disparu.

Première application le 2026-11-17 : le launcher Android Encre (`DVP-Th30/DVPLauncher`) embarque `libtallylight_ffi.so`, s'est appairé avec serveurdvp et reçoit les états réels en connexion directe. Depuis le 2026-11-19, son onglet « agent » les affiche : sessions rangées par état, ce que fait ou demande l'agent, plusieurs machines, signal d'un agent bloqué hors de l'onglet et lien vers la session dans l'app Claude (voir le `docs/ARCHITECTURE.md` d'Encre). Encre intègre encore la bibliothèque par son script `tools/tallylight.sh`, pas par l'AAR.

Fiabilité de l'état (2026-11-18, décision D9) : les sessions sont conservées dans `sessions.json` et relues au démarrage ; `claude_local.rs` lit les fiches de session de Claude Code et `Store::reconcile` les rapproche toutes les 5 secondes (sessions déjà ouvertes, sessions mortes, sessions fantômes annoncées sans processus, interruptions, titre). Vérifié sur un démon de test : les trois sessions vivantes du serveur apparaissent sans aucun événement, avec leur titre, et une vraie demande d'autorisation (`PermissionRequest`) affiche « attend une réponse ».

Limites connues :
- Poids de la bibliothèque Android : 7,2 Mo, 3,4 Mo une fois compressée (10,1 Mo au départ). Gagné le 2026-11-18 : iroh sans métriques ni ouverture de ports côté écran, générateur de liaisons sorti dans `tallylight-bindgen` (8,1 Mo), puis profil `embed` en `opt-level = "z"`, LTO et une seule unité de compilation (7,2 Mo). Reste possible : `panic = "abort"` retirerait les 2 Mo de tables de déroulement, mais un incident dans la bibliothèque ferait alors tomber l'application hôte ; écarté.
- L'interruption par Échap est déduite du statut de la fiche de session ; pas encore observée en vrai.
- Machine hors ligne au lancement du démon : les sessions sont suivies normalement, mais `tallylight pair` et `tallylight watch` sans argument échouent (après 15 s d'attente) tant que le réseau n'a pas été joint une première fois (décision D10).
- Démon arrêté : Claude Code affiche une erreur de hook non bloquante dans sa transcription.
- Codex : pas d'état `failed`, session visible seulement à partir de son premier message, et session d'un processus tué affichée jusqu'à 12 heures dans son dernier état (D17).

## Flux
1. L'agent déclenche un hook (Claude Code : hook de type `http`, POST local vers le cœur).
2. Le cœur traduit l'événement en état (`working`, `input-required`, `completed`, `failed`, `canceled`).
3. Le cœur pousse l'état aux appareils appairés par une connexion chiffrée de bout en bout (iroh).
4. L'écran affiche. Plus tard : il renvoie une réponse (autorisation, message).

Deux entrées produisent le même format : les hooks (aucune coopération de l'agent requise) et une entrée directe pour les harnais qui veulent publier leur état eux-mêmes.

## Périmètre de la première version
- Claude Code d'abord, en lecture seule ; Codex ajouté le 2026-11-22 (D17).
- Linux et macOS pour le cœur (Windows x86_64 ajouté le 2026-11-22, décision D16) ; premier écran : une commande de terminal ; démo : pastille dans le launcher Android Encre.
- Hors périmètre : iOS, les agents autres que Claude Code et Codex, approbation à distance, voix, notifications sur téléphone verrouillé.

## Premier jalon
Prouver le point le plus risqué : deux machines (serveurdvp et un téléphone Android) s'appairent par code ou QR et s'échangent un état via iroh.

Prototype jetable : `crates/tallylight-proto` (`listen` affiche un ticket et pousse des états simulés ; `connect <ticket>` les affiche avec le chemin utilisé).

Résultats au 2026-11-17 :
- serveurdvp ↔ serveurdvp : appairage par ticket, états reçus, chemin direct (1 à 5 ms).
- serveurdvp ↔ Galaxy A36 sur le même Wi-Fi, binaire lancé par adb : états reçus, chemin direct (8 à 13 ms après une première ligne à 757 ms).
- Même montage, chemin direct interdit côté téléphone (`TALLYLIGHT_RELAY_ONLY=1`) : états reçus par un relais public, 108 ms stables côté téléphone.
- 2026-11-18, téléphone sur un Wi-Fi tiers (bureau, adresse privée 10.x), application Encre : états reçus, lien d'abord par relais puis direct en moins de 20 s. Réserve : Tailscale était actif sur le téléphone (nécessaire pour adb), donc le chemin « direct » a pu emprunter l'interface Tailscale plutôt que traverser les deux box ; Tailscale éteint, le propriétaire a lu « lien direct » sur la page « réglages » d'Encre : confirmé ensuite par le journal du cœur, qui note désormais le chemin réel de chaque écran (`journalctl --user -u tallylight | grep screen`) : relais pendant l'établissement, puis direct vers une adresse IPv6 publique du téléphone (ni Tailscale ni relais), 43 à 50 ms. Relevé du 2026-11-18 à 11:05, téléphone passant de la 5G au Wi-Fi d'entreprise (pare-feu) sans Tailscale : relais 5 s, puis direct en IPv6 et en IPv4 sur la 5G, puis direct en IPv4 vers l'adresse publique du bureau 25 s après le retour au Wi-Fi — donc traversée de la traduction d'adresses de l'opérateur et de celle de l'entreprise, et changement de réseau sans coupure de la connexion.
- Pas encore testé : 4G, traversée du réseau d'un opérateur sans Tailscale, appairage par QR depuis une application.

Constats :
- Le ticket fait 261 caractères : l'appairage passera par un QR ou un code court.
- Hors d'une application, iroh lève une erreur non bloquante sur Android (`ndk-context` : « android context was not initialized ») : dans une application, le contexte Android doit être fourni à l'initialisation.
- Binaire Android optimisé : 20 Mo (non réduit) ; à mesurer une fois réduit (`strip`, `opt-level = "z"`, LTO).

## Exemples (2026-11-22)
- `examples/menubar/tallylight.5s.sh` : barre de menus macOS (SwiftBar ou xbar), lit `GET /sessions` du démon local avec `curl` et `jq`. Sortie vérifiée à la main, pas dans SwiftBar lui-même.
- `crates/tallylight-client/examples/needs_you.rs` : écran distant minimal en Rust ; garde sa clé et le ticket dans `.tallylight-example/`. Vérifié de bout en bout contre un démon d'essai.
