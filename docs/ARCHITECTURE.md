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
- `crates/tallylight-core` — modèle partagé : `State` (vocabulaire A2A), `Session`, `Snapshot`, et `claude::change` qui traduit un hook de Claude Code en changement d'état (fonction pure, testée).
- `crates/tallylight` — le binaire :
  - `tallylight daemon [--port N]` : reçoit les hooks sur `http://127.0.0.1:47700/hooks/claude`, tient les sessions (`store.rs`), sert `GET /sessions`, et pousse un instantané JSON par ligne à chaque écran appairé connecté par iroh (ALPN `tallylight/state/1`). Données dans `~/.local/share/tallylight/` : `key` (identité, donc ticket stable), `ticket`, `devices.json` (écrans autorisés).
  - `tallylight watch [appairage]` : écran terminal ; sans argument, suit cette machine (le démon local lui fournit l'appairage). Les sessions qui attendent l'utilisateur passent en tête. L'écran garde sa propre identité (`screen-key`) et la liste des machines appairées (`machines`).
  - `tallylight pair` : affiche un QR et une chaîne d'appairage à usage unique ; `tallylight devices` et `tallylight unpair <identité>` gèrent les écrans autorisés (`devices.rs`, décision D8). Ces commandes parlent au démon par son port local (`local.rs`).
  - `tallylight hooks [--port N]` : affiche le bloc `hooks` (type `http`, délai 2 s) à fusionner dans les réglages de Claude Code. Rien n'est installé automatiquement pour l'instant.
- `crates/tallylight-client` — la bibliothèque d'écran : `Screen::new(clé, nom)`, `screen.watch(appairage)` puis `watch.next()` pour chaque instantané. `tallylight watch` repose dessus. Le protocole partagé (`Hello`, `Welcome`, ALPN) vit dans `tallylight-core::protocol`.
- `crates/tallylight-ffi` — liaisons pour les applications, générées par UniFFI (Kotlin aujourd'hui, Swift ensuite) : objet `Tallylight(data_dir, name)` avec `start(pairing, listener)`, `stop()`, `paired()` ; l'application reçoit `on_snapshot` et `on_link` (direct, relais, non appairé, hors ligne). La bibliothèque garde l'identité de l'écran et le ticket de la machine, et se reconnecte seule (attente de 2 à 60 s). Sur Android, `dev.tallylight.Android.init(context)` fournit le contexte dont iroh a besoin pour lire les serveurs DNS.
- `crates/tallylight-proto` — prototype réseau jetable du premier jalon.

Correspondance Claude Code → état : `UserPromptSubmit`, `PreToolUse`, `PostToolUse` → `working` ; `PermissionRequest` et `Notification` (`permission_prompt`, `elicitation_*`) → `input-required` ; `Stop` → `completed` ; `StopFailure` → `failed` ; `SessionEnd` → session retirée. Les `Stop` de sous-agents sont ignorés.

Vérifié le 2026-11-17 avec une vraie session (`claude -p --settings <fichier de hooks>`, outil Read) : l'écran a affiché au travail → au travail (Read) → fini, puis la session a disparu.

Première application le 2026-11-17 : le launcher Android Encre (`DVP-Th30/DVPLauncher`) embarque `libtallylight_ffi.so`, s'est appairé avec serveurdvp et reçoit les états réels en connexion directe (journal seulement, pas encore d'interface).

Fiabilité de l'état (2026-11-18, décision D9) : les sessions sont conservées dans `sessions.json` et relues au démarrage ; `claude_local.rs` lit les fiches de session de Claude Code et `Store::reconcile` les rapproche toutes les 5 secondes (sessions déjà ouvertes, sessions mortes, interruptions, titre). Vérifié sur un démon de test : les trois sessions vivantes du serveur apparaissent sans aucun événement, avec leur titre, et une vraie demande d'autorisation (`PermissionRequest`) affiche « attend une réponse ».

Limites connues :
- Poids : la bibliothèque Android fait 10 Mo (profil `embed`, `opt-level = "s"`, symboles retirés) ; l'APK d'Encre passe de 2,7 à 13 Mo. À réduire.
- Une seule machine par écran côté `tallylight-ffi`.
- L'interruption par Échap est déduite du statut de la fiche de session ; pas encore observée en vrai.
- Démon arrêté : Claude Code affiche une erreur de hook non bloquante dans sa transcription.

## Flux
1. L'agent déclenche un hook (Claude Code : hook de type `http`, POST local vers le cœur).
2. Le cœur traduit l'événement en état (`working`, `input-required`, `completed`, `failed`, `canceled`).
3. Le cœur pousse l'état aux appareils appairés par une connexion chiffrée de bout en bout (iroh).
4. L'écran affiche. Plus tard : il renvoie une réponse (autorisation, message).

Deux entrées produisent le même format : les hooks (aucune coopération de l'agent requise) et une entrée directe pour les harnais qui veulent publier leur état eux-mêmes.

## Périmètre de la première version
- Claude Code seul, en lecture seule.
- Linux et macOS pour le cœur ; premier écran : une commande de terminal ; démo : pastille dans le launcher Android Encre.
- Hors périmètre : Windows, iOS, autres agents, approbation à distance, voix, notifications sur téléphone verrouillé.

## Premier jalon
Prouver le point le plus risqué : deux machines (serveurdvp et un téléphone Android) s'appairent par code ou QR et s'échangent un état via iroh.

Prototype jetable : `crates/tallylight-proto` (`listen` affiche un ticket et pousse des états simulés ; `connect <ticket>` les affiche avec le chemin utilisé).

Résultats au 2026-11-17 :
- serveurdvp ↔ serveurdvp : appairage par ticket, états reçus, chemin direct (1 à 5 ms).
- serveurdvp ↔ Galaxy A36 sur le même Wi-Fi, binaire lancé par adb : états reçus, chemin direct (8 à 13 ms après une première ligne à 757 ms).
- Même montage, chemin direct interdit côté téléphone (`TALLYLIGHT_RELAY_ONLY=1`) : états reçus par un relais public, 108 ms stables côté téléphone.
- 2026-11-18, téléphone sur un Wi-Fi tiers (bureau, adresse privée 10.x), application Encre : états reçus, lien d'abord par relais puis direct en moins de 20 s. Réserve : Tailscale était actif sur le téléphone (nécessaire pour adb), donc le chemin « direct » a pu emprunter l'interface Tailscale plutôt que traverser les deux box ; Tailscale éteint, le propriétaire a lu « lien direct » sur la page « réglages » d'Encre : la traversée des deux box semble donc fonctionner seule (observation à l'écran, non recoupée par le journal).
- Pas encore testé : 4G, traversée du réseau d'un opérateur sans Tailscale, appairage par QR depuis une application.

Constats :
- Le ticket fait 261 caractères : l'appairage passera par un QR ou un code court.
- Hors d'une application, iroh lève une erreur non bloquante sur Android (`ndk-context` : « android context was not initialized ») : dans une application, le contexte Android doit être fourni à l'initialisation.
- Binaire Android optimisé : 20 Mo (non réduit) ; à mesurer une fois réduit (`strip`, `opt-level = "z"`, LTO).
