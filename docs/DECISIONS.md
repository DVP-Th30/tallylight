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

## D8 — Appairage : liste d'appareils autorisés et code à usage unique (2026-11-17)
Chaque écran a une identité réseau (clé publique iroh), prouvée par la connexion chiffrée elle-même. Le cœur ne sert que les identités de sa liste (`devices.json`). Pour y entrer, un écran présente une fois un code tiré au hasard par `tallylight pair` (10 octets, valable 5 minutes, un seul usage), transmis avec le ticket dans un QR ou une chaîne « ticket#code ». Ensuite l'écran est reconnu sans code ; `tallylight unpair` le retire.
Le ticket seul ne donne donc aucun droit : il dit seulement où joindre la machine.
Écarté : le ticket comme secret partagé (quiconque le voit lit l'état, et on ne peut pas retirer un seul appareil) ; un mot de passe commun (même défaut) ; une validation manuelle sur la machine à chaque appairage (impossible sur un serveur sans écran).
Limites assumées : le port local du démon fait confiance à tout processus de la machine (un autre utilisateur de la même machine pourrait demander un code) ; à durcir par un socket réservé au propriétaire si des machines partagées sont visées.

## D9 — Les fiches de session de Claude Code, en complément des hooks (2026-11-18)
Les hooks seuls laissent trois trous : les sessions ouvertes avant le démon sont invisibles, une session tuée sans événement de fin reste affichée, et une interruption par Échap ne déclenche aucun hook (la session reste « au travail »). Claude Code tient une fiche par processus dans `~/.claude/sessions/<pid>.json` (identifiant de session, dossier, nom, statut `busy` / `idle`). Toutes les 5 secondes, le démon rapproche ses sessions de ces fiches : une session vivante inconnue apparaît, une session dont le processus est mort disparaît, et un tour « au travail » dont le processus est revenu au repos depuis plus de 3 secondes sans hook de fin passe en `canceled`. Le nom de la session, quand l'utilisateur ou l'agent en a donné un, devient son titre.
Ce format est interne à Claude Code et peut changer sans préavis : il complète les hooks sans les remplacer, toute fiche illisible est ignorée, et rien d'autre n'y est lu (en particulier pas les fichiers `.key` voisins).
Écarté : lire la transcription de la session pour y chercher la marque d'interruption (c'est lire le contenu des conversations) ; un simple délai sans événement (un outil long, comme une compilation, n'émet rien pendant des minutes).
Non vérifié : la détection d'interruption repose sur le passage du statut à `idle` ; elle est couverte par les tests mais n'a pas encore été observée sur un vrai appui sur Échap.

## D10 — Le port local s'ouvre avant le réseau (2026-11-18)
Le démon ouvrait son port local après avoir joint le réseau iroh, attente sans limite de temps. Deux défauts : une commande lancée juste après le démarrage trouvait porte close (constaté à la première installation sur le Mac), et une machine lancée hors ligne refusait tous les hooks jusqu'au retour du réseau, alors que suivre les sessions est purement local. Le port est maintenant ouvert en premier ; le réseau est joint en tâche de fond et le ticket n'existe qu'ensuite. Seul l'appairage a besoin du ticket : `POST /pair` l'attend 15 secondes puis répond par une erreur lisible.
Vérifié le 2026-11-18 sur un démon de test privé de réseau (`sandbox-exec`) : hooks et `/sessions` répondent, `pair` explique qu'il attend le réseau ; en ligne, port ouvert en 100 ms et `pair` servi en 0,35 s.
Écarté : borner l'attente du réseau puis continuer sans ticket (l'appairage resterait cassé jusqu'au redémarrage) ; garder l'ordre et ne faire patienter que `install` (ne règle pas le cas hors ligne).
Limite : le ticket est calculé une fois, à la première connexion au réseau, comme avant.

## Questions ouvertes
- Envoyer un message à une session Claude Code déjà ouverte (conditionne la voix) : Remote Control sans API, channels en aperçu, socket de boîte de réception au format non documenté. À valider par un essai.
- Comportement de Claude Code quand un hook `PermissionRequest` dépasse son délai.
- Notifications Android sans compte Google (UnifiedPush/ntfy) : non vérifié.
