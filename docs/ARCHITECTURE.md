# Architecture

État au 2026-11-17 : cadrage et prototype réseau. En code : le vocabulaire d'états (`crates/tallylight-core`) et un prototype jetable (`crates/tallylight-proto`).

## Promesse
Le lien clé en main entre n'importe quel agent de code et n'importe quel écran : un binaire, un format d'état standard, un réseau privé intégré, des clients pour chaque plateforme.

## Composants visés
- **Cœur (`tallylightd`)** — un binaire par machine. Installe les hooks des agents présents, reçoit leurs événements, tient l'état des sessions et le diffuse aux appareils appairés.
- **Bibliothèque cliente** — le même code Rust compilé pour Linux, macOS, Windows, Android, iOS et le navigateur. Une application l'intègre pour recevoir « quelles sessions, dans quel état ».
- **Ponts simples** — sortie HTTP locale et sortie Bluetooth compatible avec le protocole d'objets d'Anthropic (`claude-desktop-buddy`), pour un écran ESP32 qui n'embarque pas le réseau.
- **Réveil (optionnel, plus tard)** — petit service à déployer par l'éditeur d'une application, avec sa propre clé FCM/APNs, pour notifier un téléphone en veille.

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
- Pas encore testé : téléphone hors du réseau local (relais, 4G), appairage par QR, intégration dans une application Android.

Constats :
- Le ticket fait 261 caractères : l'appairage passera par un QR ou un code court.
- Hors d'une application, iroh lève une erreur non bloquante sur Android (`ndk-context` : « android context was not initialized ») : dans une application, le contexte Android doit être fourni à l'initialisation.
- Binaire Android optimisé : 20 Mo (non réduit) ; à mesurer une fois réduit (`strip`, `opt-level = "z"`, LTO).
