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
  - `tallylight daemon [--port N]` : reçoit les hooks sur `http://127.0.0.1:47700/hooks/claude`, tient les sessions (`store.rs`), sert `GET /sessions`, et pousse un instantané JSON par ligne à chaque écran connecté par iroh (ALPN `tallylight/state/0`). Identité réseau et ticket dans `~/.local/share/tallylight/` (`key`, `ticket`) : le ticket ne change pas d'un lancement à l'autre.
  - `tallylight watch [ticket]` : écran terminal ; sans ticket, lit celui du démon local. Les sessions qui attendent l'utilisateur passent en tête.
  - `tallylight hooks [--port N]` : affiche le bloc `hooks` (type `http`, délai 2 s) à fusionner dans les réglages de Claude Code. Rien n'est installé automatiquement pour l'instant.
- `crates/tallylight-proto` — prototype réseau jetable du premier jalon.

Correspondance Claude Code → état : `UserPromptSubmit`, `PreToolUse`, `PostToolUse` → `working` ; `PermissionRequest` et `Notification` (`permission_prompt`, `elicitation_*`) → `input-required` ; `Stop` → `completed` ; `StopFailure` → `failed` ; `SessionEnd` → session retirée. Les `Stop` de sous-agents sont ignorés.

Vérifié le 2026-11-17 avec une vraie session (`claude -p --settings <fichier de hooks>`, outil Read) : l'écran a affiché au travail → au travail (Read) → fini, puis la session a disparu.

Limites connues :
- Une interruption par l'utilisateur (Échap) ne déclenche aucun hook : la session reste « au travail ».
- Quiconque possède le ticket peut lire l'état : la liste des appareils autorisés reste à faire.
- Les sessions ouvertes avant le démarrage du démon n'apparaissent qu'à leur prochain événement ; rien n'est conservé à l'arrêt du démon.
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
- Pas encore testé : téléphone réellement hors du réseau local (4G, Wi-Fi tiers) et traversée du réseau d'un opérateur, appairage par QR, intégration dans une application Android. Le débogage sans fil d'Android exige un Wi-Fi : un test en 4G par adb demande de passer adb en port fixe, ou de lancer le binaire depuis le téléphone.

Constats :
- Le ticket fait 261 caractères : l'appairage passera par un QR ou un code court.
- Hors d'une application, iroh lève une erreur non bloquante sur Android (`ndk-context` : « android context was not initialized ») : dans une application, le contexte Android doit être fourni à l'initialisation.
- Binaire Android optimisé : 20 Mo (non réduit) ; à mesurer une fois réduit (`strip`, `opt-level = "z"`, LTO).
