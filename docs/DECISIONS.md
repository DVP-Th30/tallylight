# Décisions

## D1 — Une bibliothèque, pas une application (2026-11-17)
Dix projets « démon + hooks + réseau pair-à-pair + QR » sont apparus en octobre et novembre 2026, tous des applications de télécommande d'agents. Aucun n'est une brique réutilisable. On reste strictement sur la brique ; les applications (pastille Encre, commande de terminal, objet de bureau) sont des exemples et des démos.
Écarté : une application de suivi de plus, un objet d'abord (risque matériel avant d'avoir validé la couche).

## D2 — Vocabulaire d'états repris d'A2A, pas de standard maison (2026-11-17)
Les états suivent `TaskState` d'A2A 1.0 : `working`, `input-required`, `completed`, `failed`, `canceled`. A2A est sous gouvernance neutre (Linux Foundation), OpenTelemetry reprend ses valeurs et MCP Tasks utilise presque les mêmes mots.
Écarté : inventer un format (un acteur seul n'impose pas un standard) ; AG-UI (trop fin, piloté par un éditeur) ; OpenTelemetry (télémétrie après coup).

## D3 — Rust (2026-11-17)
Un seul fichier sans dépendance, peu de mémoire, le même cœur pour ordinateur, téléphone et navigateur, et la bibliothèque réseau retenue (D4) est native en Rust. La performance n'est pas l'enjeu (quelques événements par minute) ; le format de livraison l'est.
Limite assumée : le langage ne protège pas de la copie. Ce qui protège, c'est d'être la brique que les autres intègrent.

## D4 — Réseau embarqué : iroh (2026-11-17)
iroh 1.x : API et protocole annoncés stables, licence MIT/Apache, appairage sans compte, liaisons officielles Kotlin et Swift, navigateur et ESP32 possibles.
Écarté : Tailscale embarqué (compte obligatoire, bibliothèque en Go, version Rust en chantier) ; libp2p (beaucoup d'assemblage, pas de liaisons mobiles officielles) ; WebRTC (signalisation et TURN à fournir).
Conséquences à respecter :
- iroh ne réveille pas un téléphone en veille : les notifications passent par FCM/APNs, donc par un service optionnel tenu par l'éditeur de l'application (sa clé, son compte développeur).
- Les relais publics de l'éditeur d'iroh sont réservés au développement : prévoir un relais auto-hébergeable.
- Connexion permanente coûteuse sur mobile (ticket iroh #4651) : fermer la connexion en arrière-plan.
- iroh-gossip et iroh-docs sont encore en 0.x : écrire un petit protocole maison sur flux QUIC.

## D5 — Collecte par les hooks, sans relancer les agents (2026-11-17)
Les sessions sont suivies là où elles tournent, par les hooks de l'agent. Harness impose tmux et ne suit que les agents qu'il lance : c'est sa principale limite.
Claude Code : hooks de type `http` vers le cœur ; état immédiat par `UserPromptSubmit`/`PreToolUse` (au travail), `PermissionRequest` (attend), `Stop` (fini).

## D6 — Open source gratuit, licence MIT OR Apache-2.0 (2026-11-17)
But : visibilité, apprentissage, et un revenu éventuel à terme (service de réveil ou relais hébergés). Double licence, comme iroh et l'écosystème Rust.

## D7 — Dépôt privé jusqu'à la première démo (2026-11-17)
Rendu public le jour où une démo fonctionne. Nom `tallylight` : le voyant des caméras de télévision qui indique laquelle est à l'antenne ; libre sur crates.io à cette date.

## Questions ouvertes
- Envoyer un message à une session Claude Code déjà ouverte (conditionne la voix) : Remote Control sans API, channels en aperçu, socket de boîte de réception au format non documenté. À valider par un essai.
- Comportement de Claude Code quand un hook `PermissionRequest` dépasse son délai.
- Notifications Android sans compte Google (UnifiedPush/ntfy) : non vérifié.
