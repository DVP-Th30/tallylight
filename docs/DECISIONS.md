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

## D17 — Codex : hooks `command` dans `hooks.json`, relayés par `tallylight hook codex` (2026-11-22)
Deuxième agent suivi. Sources lues le 2026-11-22, la source officielle du jour l'emportant sur l'étude du 2026-11-17 (corrigée) :
- la doc officielle, https://developers.openai.com/codex/hooks (qui redirige vers https://learn.chatgpt.com/docs/hooks) : ni datée ni versionnée ;
- le dépôt https://github.com/openai/codex, commit `a0995c2b2b68248960c69cb812b5f4898dac7e4c` (2026-11-22), les mêmes fichiers existant à l'étiquette `rust-v0.171.0` (2026-11-20) : `codex-rs/hooks/src/schema.rs` (corps de chaque événement), `codex-rs/protocol/src/hooks.rs` et `codex-rs/config/src/hook_config.rs` (format de `hooks.json`), `codex-rs/hooks/src/engine/discovery.rs` (chargement, approbation), `codex-rs/hooks/src/engine/command_runner.rs` (lancement par le shell), `codex-rs/hooks/src/events/` (`interrupt.rs`, `session_end.rs`, `model_request.rs`), `codex-rs/core/tests/suite/hooks.rs` (arguments de `Bash` et d'`apply_patch`) ;
- le paquet npm `@openai/codex` 0.171.0 (2026-11-21), dont le binaire macOS a servi aux essais.

Ce qu'il faut savoir de Codex :
- Les hooks se déclarent dans `~/.codex/hooks.json` (ou `$CODEX_HOME`), même forme que chez Claude Code, ou dans la table `[hooks]` de `config.toml`. Deux types seulement : `command` et `mcp_tool`. Pas de `http` : un type inconnu rend tout le fichier illisible pour Codex.
- Une commande est lancée par `$SHELL -lc`, reçoit l'événement en JSON sur son entrée, et une sortie en succès sans rien écrire vaut « rien à dire ».
- Un hook nouveau ou modifié ne tourne pas tant que l'utilisateur ne l'a pas approuvé : Codex le propose à son lancement (« Hooks need review »), ou par `/hooks`. L'approbation est notée par Codex dans `config.toml` (`[hooks.state]`), avec l'empreinte du hook.
- Corps des événements : ceux de Claude Code (`session_id`, `cwd`, `hook_event_name`, `tool_name`, `tool_input`, `last_assistant_message`, `transcript_path`), plus `model` et `turn_id`. Outils : `Bash` (`tool_input.command`, et `description` pour certaines demandes d'autorisation), `apply_patch` (`command` contient le correctif), outils MCP.
- Événements : `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PermissionRequest`, `PostToolUse`, `Stop`, `SubagentStart`/`SubagentStop`, `PreCompact`/`PostCompact`, `PreModelRequest`/`PostModelRequest`, `Interrupt`, `SessionEnd`. Ni `Notification`, ni `StopFailure`.
- Sessions écrites dans `~/.codex/sessions/AAAA/MM/JJ/rollout-<date>-<identifiant>.jsonl`. Aucune fiche par processus comme celles de Claude Code (D9).

Choix :
- Fichier `hooks.json`, pas `config.toml` : du JSON, que `install` sait déjà modifier sans rien déranger (sauvegarde `hooks.json.before-tallylight`, hooks des autres intacts, retrait par `uninstall`, fichier supprimé s'il ne contenait que les nôtres). Modifier un TOML à la main aurait demandé une bibliothèque de plus et risqué les commentaires de l'utilisateur.
- Une commande `tallylight hook codex --port N` plutôt que `curl` : aucune dépendance, jamais d'erreur affichée dans la session (démon arrêté, entrée illisible : sortie en succès, sans un mot), délai interne de 0,8 s car Codex n'accorde qu'une seconde aux hooks `SessionEnd` et `Interrupt`. Elle retire `tool_response` avant l'envoi : la sortie des commandes ne quitte pas le processus du hook. Nos hooks se reconnaissent à la fin de leur commande ; un binaire déplacé remplace la commande au lieu de la doubler (et Codex redemande alors l'approbation, l'empreinte ayant changé).
- Hooks synchrones, pas `async` : l'ordre des événements fait l'état (un `PostToolUse` arrivé après `Stop` laisserait la session « au travail »). Coût mesuré : 12 ms par événement sur un Mac, profil de shell vide.
- Installation seulement si Codex est là (dossier `~/.codex` ou `codex` dans le `PATH`) : pas de dossier `.codex` créé chez qui ne s'en sert pas. On n'écrit jamais l'approbation à la place de l'utilisateur : c'est un garde-fou de Codex, `install` dit seulement qu'elle reste à donner.
- `SessionStart` n'est pas posé : sans fiche de session, une session qui n'a jamais travaillé ne pourrait pas être distinguée d'un processus mort (règle des fantômes de D9). Une session Codex apparaît donc à son premier message.
- Le même corps d'événement (`claude::HookEvent`, avec `model` en plus) sert aux deux agents ; `Store::apply` est commun, seuls la traduction et l'origine du modèle diffèrent.

Limites, dites telles quelles dans le README :
- Pas de `failed`. Vérifié : requête au modèle en échec, aucune relance → ni `Stop` ni autre événement de fin ; la session reste « au travail » jusqu'à l'événement suivant.
- Un Codex tué (terminal fermé, `kill`) n'envoie pas `SessionEnd` : sa session reste affichée dans son dernier état jusqu'à 12 heures (règle existante des sessions sans fiche).
- Après une autorisation accordée, l'état reste « attend une réponse » jusqu'à la fin de la commande (`PostToolUse`), comme pour Claude Code.
- Si `config.toml` déclare aussi des hooks, Codex charge les deux et le signale au lancement ; `install` le dit.
- Pas de Codex sous Windows (ajouté à la fusion avec D16) : Codex lance ses hooks par `$SHELL -lc` sous Unix, et la façon dont il le fait sous Windows n'a pas été lue ni essayée. `install` y saute donc les hooks de Codex et le dit, plutôt que d'écrire une commande qui pourrait échouer à chaque événement.

Vérifié le 2026-11-22 sous macOS, avec le vrai binaire Codex 0.171.0 branché sur un faux modèle local (fournisseur `model_providers` pointé sur `127.0.0.1`, API Responses simulée, aucun compte OpenAI), un démon d'essai et des dossiers temporaires :
- `codex exec`, hooks approuvés pour l'essai par `--dangerously-bypass-hook-trust` : `UserPromptSubmit`, `PreToolUse` (`Bash`), `PostToolUse`, `Stop`, `SessionEnd` reçus, corps conformes au schéma ; l'écran passe par au travail (Bash, « echo ») puis la session disparaît ;
- sans approbation : aucun hook ne tourne, et l'interface du terminal affiche « Hooks need review, 7 hooks are new or changed » ;
- interface du terminal dans un pseudo-terminal, commande demandant une élévation : `PermissionRequest` reçu avec `description`, état « attend une réponse », puis `PostToolUse` et `Stop` après accord ;
- `SIGINT` pendant une commande : `Interrupt` puis `SessionEnd`.
Non vérifié : une session avec un vrai modèle d'OpenAI ; Linux ; `apply_patch` et les sous-agents en session réelle (corps tirés des tests de Codex) ; l'approbation menée jusqu'au bout dans l'interface (l'invite a été vue, pas validée) ; la sortie normale de l'interface du terminal.

Écarté :
- `notify` de `config.toml` (ancien mécanisme) : ne signale que la fin d'un tour.
- Un `failed` tiré de `PostModelRequest` (`status: failed`) : essayé, il est émis à chaque tentative (six fois en 26 s avec les relances par défaut), pas à l'abandon du tour. Il faudrait poser deux hooks de plus par requête au modèle et temporiser dans le démon pour ne pas faire clignoter l'état pendant les relances. À reprendre si l'absence de `failed` gêne à l'usage.
- Lire les fichiers `rollout-*.jsonl` pour savoir si une session vit encore : format interne, et un fichier qui ne bouge plus ne dit pas si le processus est mort ou au repos.

## D18 — Swift : un paquet local, bibliothèque statique dans un xcframework (2026-11-22)
Les liaisons Swift sortent du même crate `tallylight-ffi` que les liaisons Kotlin. `apple/` est un paquet Swift : une cible binaire `TallylightFFI.xcframework` (la bibliothèque Rust et son en-tête C, une tranche par plateforme : iPhone, simulateur Apple Silicon, macOS Apple Silicon et Intel) et une cible `TallylightKit` qui ne contient que le fichier généré par UniFFI. `apple/build-xcframework.sh` produit les deux ; ils ne sont pas dans le dépôt. Une version publie le paquet entier en archive ; l'application l'ajoute comme paquet local.
- Bibliothèque statique : rien à embarquer ni à signer à part dans l'application, et l'éditeur de liens retire ce qui ne sert pas. Limite assumée : une application qui lie déjà une autre bibliothèque Rust statique aura des symboles en double (chacune embarque la bibliothèque standard).
- `cargo rustc --crate-type staticlib` dans le script, plutôt que `staticlib` dans `Cargo.toml` : la compilation Android et les tests ne produisent pas une archive dont ils n'ont pas l'usage. Le code propre à Android était déjà réservé à cette cible (`cfg(target_os = "android")`) : rien n'y change.
- Module Swift `TallylightKit`, pas `Tallylight` : un module qui porte le nom de sa classe principale ne peut plus servir à désigner ses types (`Tallylight.Session` chercherait dans la classe), or `Session`, `Machine` et `Link` sont des noms qu'une application a des chances d'avoir déjà.
- En-tête et carte de module dans `Headers/TallylightFFI/` : deux xcframeworks qui posent chacun un `module.modulemap` à la racine de leurs en-têtes se heurtent dans Xcode.
- iroh reste compilé sans ses options par défaut (comme pour Android) : ni ouverture de ports sur la box, ni l'envoi groupé de paquets qui passe par des API privées d'Apple (`fast-apple-datapath`), à éviter pour l'App Store.
- Versions minimales : iOS 15, macOS 12.
- iroh 1.4.2 au minimum (au lieu de 1.4.1), pour tout l'espace de travail : avec 1.4.1, la dépendance `netdev` 0.46 ne compile pas pour iOS (elle utilise `libc::in6_ifreq`, que `libc` ne définit que pour macOS ; constaté en CI le 2026-11-22). La version corrective 1.4.2 passe à `netdev` 0.47, qui réserve ce code à macOS. Le démon et la bibliothèque Android changent donc aussi de version d'iroh.
Écarté : un `Package.swift` à la racine avec une cible binaire distante (`url` et empreinte), qui permettrait d'ajouter le dépôt par son adresse : il faut alors committer le fichier Swift généré et inscrire dans le manifeste l'empreinte d'une archive qui n'existe qu'après la publication, soit une version en deux temps. À faire quand une application iOS consommera vraiment le paquet. Écartés aussi : CocoaPods (dépôt central en lecture seule à partir de décembre 2026) ; un framework dynamique (à embarquer et signer, sans gain ici) ; la tranche Intel du simulateur par défaut (une compilation de plus pour des Mac qui disparaissent ; `IOS_SIM_TARGETS` l'ajoute).
Vérifié et non vérifié : voir `docs/INFRA.md`, « Compiler pour iOS et macOS ». En bref, le 2026-11-22 : essai de bout en bout réussi sur macOS et dans le simulateur iOS ; aucune application iOS réelle ne l'a encore utilisé, et la question de l'autorisation « Réseau local » d'iOS reste à trancher sur un appareil.

## D19 — Proxy sortant lu dans l'environnement ; sessions hébergées en ligne (2026-11-23)
Essai du 2026-11-23 dans une session Claude Code hébergée par Anthropic (« sur le web »), tallylight 0.1.1 :
- Par défaut, le proxy de l'environnement refuse notre site (403, hôte hors liste) : rien ne s'installe. Avec un accès réseau élargi, l'installation passe.
- `tallylight install --no-service` puis `tallylight daemon` lancé à la main (pas de systemd) : les hooks écrits dans `~/.claude/settings.json` en cours de session sont pris en compte, la session apparaît dans `/sessions` avec son état et son outil.
- Le démon ne rejoignait aucun relais : il ignorait le proxy et tentait de sortir en direct. `tallylight pair` échouait.

Décision : `Relays::apply` appelle `proxy_from_env()` d'iroh (variables `HTTP_PROXY`, `http_proxy`, `HTTPS_PROXY`, `https_proxy`, dans cet ordre), pour le démon comme pour les écrans. Sans variable, rien ne change. Vérifié en local avec un proxy `CONNECT` d'essai : le démon rejoint un relais par le proxy et délivre une chaîne d'appairage.
- Derrière un proxy, tout passe par un relais : pas de chemin direct en UDP.
- `NO_PROXY` n'est pas lu par iroh.
- iroh demande au proxy les relais avec un point final (`euc1-1.relay.n0.iroh.link.:443`) : une liste d'hôtes autorisés stricte peut ne pas le reconnaître. Non vérifié.
- Bout en bout vérifié le 2026-11-23 avec la 0.1.2 dans une session hébergée (accès réseau complet) : journal `proxy: taken from the environment…` puis `online through https://use1-1.relay.n0.iroh.link./`, `tallylight pair` réussi, la machine `vm` et sa session vues sur un téléphone Android, lien « relais ». La machine s'appelle `vm` et la session `user` (dossier `/home/user`, pas de titre) : rien ne dit de quelle session il s'agit, `tallylight name` à prévoir dans le script de démarrage.
- L'installation disparaît avec l'environnement : pour un usage réel, elle va dans le script de démarrage de l'environnement.

Deux corrections venues du même essai : le journal du démon dit au bout de 20 s qu'aucun relais n'est joint (il restait muet), et les erreurs sont affichées par le binaire lui-même (renvoyées depuis `main`, elles traînaient une trace d'appels illisible dès que `RUST_BACKTRACE` est défini).

## D20 — Tâches de fond : le tour reste « fini », un compteur voyage à part (2026-11-23)
Constat : un agent qui lance une commande en arrière-plan puis termine son tour s'affichait `completed`, alors qu'il reprend seul à la fin de la commande. `Stop` de Claude Code porte `background_tasks` (et `session_crons`) ; observé le 2026-11-23 sur une session d'essai : une entrée `{"type":"shell","status":"running",…}`, puis `UserPromptSubmit` au réveil, puis un `Stop` à liste vide.

Décision : la session garde l'état `completed` et gagne un champ `background_tasks`, le nombre d'entrées en `running` au dernier `Stop` de la session elle-même, remis à zéro à `UserPromptSubmit`. Champ ajouté sans nouvelle version du protocole (les champs inconnus sont ignorés).
- Écarté : rester `working` tant qu'une tâche tourne. Claude Code ne dit pas si la tâche réveillera l'agent ou ne finit jamais (serveur de développement, surveillance) : la session resterait « au travail » indéfiniment alors que l'agent attend l'utilisateur.
- Écarté : un nouvel état. Il change le vocabulaire (A2A) que chaque écran doit apprendre.
- `session_crons` (réveils programmés) n'est pas compté : entre deux réveils la session est bien au repos.
- Le nombre date du dernier `Stop` : une tâche qui se termine sans réveiller l'agent le laisserait faux jusqu'au tour suivant (non observé : la fin d'une commande de fond a réveillé l'agent).
- Codex n'envoie rien de tel : toujours zéro.

## Questions ouvertes
- Codex : obtenir `failed` (voir D17, piste `PostModelRequest`) et savoir qu'un processus Codex est mort sans événement de fin.
- Envoyer un message à une session Claude Code déjà ouverte (conditionne la voix) : Remote Control sans API, channels en aperçu, socket de boîte de réception au format non documenté. À valider par un essai.
- Comportement de Claude Code quand un hook `PermissionRequest` dépasse son délai.
- Notifications Android sans compte Google (UnifiedPush/ntfy) : non vérifié.
