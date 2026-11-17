# Architecture

État au 2026-11-17 : cadrage. Seul le vocabulaire d'états existe en code (`crates/tallylight-core`).

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
