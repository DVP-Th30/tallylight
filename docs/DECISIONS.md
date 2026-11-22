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
Sessions fantômes (ajout du 2026-11-18, constaté sur le Mac) : une session qui a seulement envoyé son hook de démarrage, sans fiche locale ni tour commencé au bout de 60 secondes, est oubliée. Deux sources observées : les lancements sans écran que Vibe Island fait depuis `~/.vibe-island/cache` (trois sessions « cache » restaient affichées 12 heures), et l'identifiant provisoire que Claude Code annonce avant de reprendre une session existante. Une session qui a travaillé garde, elle, la règle des 12 heures sans fiche. Si les fiches venaient à disparaître (format changé), une session neuve ne s'afficherait qu'à son premier message.
Non vérifié : la détection d'interruption repose sur le passage du statut à `idle` ; elle est couverte par les tests mais n'a pas encore été observée sur un vrai appui sur Échap.

## D10 — Le port local s'ouvre avant le réseau (2026-11-18)
Le démon ouvrait son port local après avoir joint le réseau iroh, attente sans limite de temps. Deux défauts : une commande lancée juste après le démarrage trouvait porte close (constaté à la première installation sur le Mac), et une machine lancée hors ligne refusait tous les hooks jusqu'au retour du réseau, alors que suivre les sessions est purement local. Le port est maintenant ouvert en premier ; le réseau est joint en tâche de fond et le ticket n'existe qu'ensuite. Seul l'appairage a besoin du ticket : `POST /pair` l'attend 15 secondes puis répond par une erreur lisible.
Vérifié le 2026-11-18 sur un démon de test privé de réseau (`sandbox-exec`) : hooks et `/sessions` répondent, `pair` explique qu'il attend le réseau ; en ligne, port ouvert en 100 ms et `pair` servi en 0,35 s.
Écarté : borner l'attente du réseau puis continuer sans ticket (l'appairage resterait cassé jusqu'au redémarrage) ; garder l'ordre et ne faire patienter que `install` (ne règle pas le cas hors ligne).
Limite : le ticket est calculé une fois, à la première connexion au réseau, comme avant.

## D11 — Relais iroh au choix, n0 par défaut (2026-11-20)
D4 réserve les relais publics de n0 au développement : chacun doit pouvoir indiquer le sien. Le démon, `tallylight watch` et la bibliothèque d'écran acceptent `--relay` (ou `TALLYLIGHT_RELAY`, ou `Tallylight.withRelay` côté application) : `n0` ou une liste d'adresses `https://…`. Sans réglage, rien ne change : n0, et un service installé identique à celui des versions précédentes.
- La machine annonce le relais joint dans son ticket : un écran la rejoint par ce relais, quel que soit le sien. Pas besoin d'aligner les réglages des deux côtés.
- Adresse limitée au schéma, à l'hôte et au port : elle finit dans le ticket montré aux écrans, donc ni identifiant, ni jeton, ni chemin.
- `install --relay` inscrit le choix dans le service ; une réinstallation sans `--relay` relit celui du service plutôt que de revenir à n0 en silence.
- `http://` est accepté pour un relais d'essai local.
Vérifié le 2026-11-20 sur le Mac avec `iroh-relay --dev` (1.4.1) : démon et écran terminal réglés sur `http://127.0.0.1:3340`, ticket joint par ce relais, sessions reçues, puis lien direct. Avec un relais injoignable, le port local reste ouvert (D10) et `pair` explique que le relais ne répond pas.
Écarté : un relais de projet tenu par tallylight (serveur à payer et à surveiller, contraire à « sans serveur ») ; un réglage dans un fichier de configuration (pas d'autre réglage persistant aujourd'hui, l'option et la variable suffisent).
Limite : pas encore de relais hébergé à recommander ni de guide d'installation d'`iroh-relay`.

## D12 — Fichiers d'état écrits d'un coup (2026-11-21)
Constaté le 2026-11-21 : Encre affichait « aucune machine appairée ». Il ne s'était plus connecté depuis le 19 alors que son identité d'écran et son autorisation sur les deux machines étaient intactes ; seule la liste `machines.json` était perdue. Cause probable (le fichier n'était pas lisible pour le prouver, application non déboguable) : la bibliothèque réécrivait la liste à chaque reconnexion par un simple `write`, chaque machine depuis sa propre tâche, et un fichier illisible était lu comme une liste vide. Android tue souvent une application en arrière-plan : une écriture interrompue laisse un fichier tronqué, et deux tâches qui écrivent ensemble s'effacent l'une l'autre.
Désormais :
- écriture dans un fichier temporaire puis renommage, pour la liste des machines de la bibliothèque comme pour tous les fichiers du démon (`write_private` : clé, ticket, écrans autorisés, sessions). Une interruption laisse l'ancien contenu entier ;
- lecture-modification-écriture de la liste sous un verrou, et pas d'écriture quand rien ne change ;
- une liste illisible est mise de côté (`machines.json.unreadable`) au lieu d'être écrasée ;
- côté démon, le fichier temporaire est créé directement en 0600 : la clé secrète n'est plus lisible par d'autres utilisateurs, même un instant (avant : création avec les droits par défaut, puis `chmod`).
Écarté : une base SQLite (dépendance lourde pour quelques lignes de JSON).
Remède immédiat appliqué : nouvel appairage d'Encre avec serveurdvp et le Mac par adb.

## D13 — Le modèle, lu dans la transcription (2026-11-21)
Demande : voir sur l'écran quel modèle utilise chaque agent. Ni les hooks ni la fiche de session (`~/.claude/sessions/<pid>.json`) ne le donnent ; la transcription si : chaque réponse de l'agent y porte `message.model`. Le démon lit donc la fin (256 Ko) de la transcription :
- à chaque hook, par le `transcript_path` que Claude Code fournit ;
- toutes les 5 s pour les sessions vivantes, au rapprochement des fiches (sessions ouvertes avant le démon, ou sans hooks), seulement si le fichier a changé. Chemin : `~/.claude/projects/<dossier>/<session>.jsonl`, où chaque caractère non alphanumérique du dossier devient `-` (vérifié sur les espaces et les points ; à défaut, le fichier de la session est cherché dans tous les dossiers de projets).
Sont ignorés : les réponses des sous-agents (`isSidechain`) et les messages fabriqués par Claude Code (`<synthetic>`). Un changement de modèle en cours de session (`/model`) apparaît à la réponse suivante.
Comme D9 : format interne de Claude Code, susceptible de changer ; une ligne illisible est ignorée et le modèle reste alors simplement inconnu. Rien d'autre que ce champ n'est lu ni envoyé.
Écarté : demander le modèle à Claude Code (aucune interface pour cela) ; le déduire des réglages (`settings.json`), qui ne reflètent ni `/model` ni `--model`.

## D14 — Anglais pour tout ce que voit l'utilisateur, français pour les notes de conception (2026-11-22)
Avant le passage en public : messages du binaire, aide, journal du démon, README, `docs/PROTOCOL.md`, fichiers de contribution et de sécurité et page de présentation sont en anglais. Les commentaires du code et les notes de conception (`ARCHITECTURE`, `DECISIONS`, `INFRA`, `ETAT-DE-L-ART`) restent en français : ce sont les documents de travail du propriétaire, et les traduire doublerait leur entretien.
Limite assumée : un contributeur non francophone lit le code sans ses commentaires. À revoir si des contributions arrivent.
Écarté : tout traduire d'un coup (gros chantier sans effet sur l'utilisateur) ; une double doc français/anglais (les deux versions divergent).

## D15 — Versions : binaires statiques par étiquette, script d'installation (2026-11-22)
Une étiquette `v*` publie quatre binaires (Linux x86_64 et arm64 liés à musl, macOS Apple Silicon et Intel) avec leur empreinte SHA-256. Linux en musl : un binaire lié à la glibc de la machine de construction refuse de se lancer sur une distribution plus ancienne. `site/install.sh` installe la dernière version dans `~/.local/bin` après vérification de l'empreinte ; `cargo install --git` reste possible.
Écarté pour l'instant : Homebrew, paquets `.deb` (à faire si la demande existe) ; cargo-dist (outil de plus pour quatre cibles) ; Windows (le service et les chemins sont propres à Unix) — levé par D16.

## D16 — Windows : tâche planifiée à l'ouverture de session, données dans `%LOCALAPPDATA%` (2026-11-22)
Le binaire se compile et tourne sous Windows x86_64 ; une cinquième cible, `x86_64-pc-windows-msvc`, sort avec chaque version (archive zip).
- **Service** : `tallylight install` inscrit une tâche planifiée `tallylight` (`schtasks /Create /XML`), déclenchée à l'ouverture de session de l'utilisateur, avec ses droits ordinaires. Elle est décrite en XML car les options de `schtasks` ne savent ni viser un seul utilisateur, ni lever les réglages par défaut du planificateur (pas de lancement sur batterie, arrêt au bout de trois jours). Relance en cas d'échec : toutes les minutes, dix fois. La tâche lance `tallylight daemon --background` : le démon se détache de sa console (pas de fenêtre ouverte en permanence) et écrit son journal dans `%LOCALAPPDATA%\tallylight\daemon.log`, remis à zéro au lancement au-delà de 1 Mo. Une copie de la tâche (`service.xml`) reste dans le dossier de données pour retrouver les relais à la réinstallation, comme le fichier de service des autres systèmes (D11).
- **Données** : `%LOCALAPPDATA%\tallylight` (propre à la machine, pas dans le profil itinérant : la clé est l'identité de cette machine). `XDG_DATA_HOME` reste prioritaire partout, pour les essais.
- **Droits des fichiers** : Windows n'a pas de mode 0600. Les fichiers héritent des droits de `%LOCALAPPDATA%`, ouvert au seul utilisateur, au système et aux administrateurs ; rien n'est durci au-delà. L'écriture d'un coup (fichier temporaire puis renommage, D12) est la même.
- **Réglages de Claude Code** : `%USERPROFILE%\.claude\settings.json` (documentation officielle, https://code.claude.com/docs/en/settings, lue le 2026-11-22), ou `CLAUDE_CONFIG_DIR`. Les hooks de type `http` sont les mêmes sur tous les systèmes.
- **Processus vivants** (D9) : `OpenProcess` puis `GetExitCodeProcess`, à la place du signal 0.
- **Chemins** : le dernier élément d'un dossier ou d'un fichier se lit avec `/` comme avec `\`, y compris sur un écran Linux qui affiche une machine Windows ; sinon le chemin entier d'un fichier partait vers les écrans. L'outil `PowerShell` de Claude Code est résumé comme `Bash`.
- **Binaire** : bibliothèque C de Microsoft liée dans le binaire (`.cargo/config.toml`), donc rien à installer à côté. `site/install.ps1` vérifie l'empreinte, installe dans `%LOCALAPPDATA%\tallylight\bin`, ajoute ce dossier au `PATH` de l'utilisateur et, pour une mise à jour, arrête la tâche le temps de remplacer le binaire (Windows verrouille un programme qui tourne).

Écarté :
- un vrai service Windows : droits d'administrateur à l'installation, et il tourne sous un compte système qui ne voit ni `%USERPROFILE%\.claude` ni les sessions de l'utilisateur, sauf à lui demander son mot de passe ;
- la clé `Run` du registre ou le dossier Démarrage : ni relance après un échec, ni arrêt (il faudrait tuer le processus par son nom), ni état à interroger ;
- `--no-service` seul, démon à lancer à la main : c'est le repli qui reste possible, pas une installation ;
- `conhost --headless` pour cacher la console : option non documentée de Windows.

Limites, et ce qui n'a pas été vu sur un vrai poste Windows (seulement sur les machines `windows-latest` de GitHub Actions) :
- aucune vraie session Claude Code n'a été suivie sous Windows : le format des fiches `sessions\<pid>.json` (D9) et le dossier des transcriptions (D13, `C:\Users\moi\projet` donnerait `C--Users-moi-projet`) sont supposés identiques à ceux de Linux et macOS ;
- à l'ouverture de session, une fenêtre de console peut apparaître un instant avant que le démon s'en détache ;
- la création de la tâche par un utilisateur sans droits d'administrateur n'est pas vérifiée (le compte des machines d'essai est administrateur) ;
- la tâche est arrêtée sèchement (`schtasks /End`), sans extinction propre du démon : sans conséquence sur les fichiers d'état (D12), l'écran voit seulement la connexion tomber ;
- pas de binaire ARM64 : le script installe le binaire x86_64, que Windows 11 ARM exécute par émulation (non essayé).

## Questions ouvertes
- Envoyer un message à une session Claude Code déjà ouverte (conditionne la voix) : Remote Control sans API, channels en aperçu, socket de boîte de réception au format non documenté. À valider par un essai.
- Comportement de Claude Code quand un hook `PermissionRequest` dépasse son délai.
- Notifications Android sans compte Google (UnifiedPush/ntfy) : non vérifié.
