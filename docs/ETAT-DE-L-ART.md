# État de l'art (étude du 2026-11-17)

Cinq recherches sur sources primaires (dépôts, docs officielles). Les points marqués « non vérifié » ne viennent pas d'une source primaire. À revérifier avant toute publication : le domaine bouge chaque semaine.

## Concurrents et voisins

| Projet | Ce qu'il fait | Limite pour notre cible |
|---|---|---|
| [AgentDeck](https://github.com/puritysb/AgentDeck) (MIT, Node/Swift) | Démon par machine, 8 agents, nombreux écrans (Stream Deck, ESP32, TRMNL, app Android), « Surface Protocol v1 » documenté | Réseau local seulement ; un seul auteur |
| [Harness](https://github.com/autonomous-ai/openharness) (MIT, TypeScript/Flutter) | 14 agents, apps bureau et mobiles, chiffrement de bout en bout, voix, objets à 149 $ et 299 $ | Compte obligatoire, relais de l'éditeur, tmux imposé, ne suit que les agents qu'il lance ; pas d'API d'état stable (issue #23) |
| [codelight](https://github.com/henrikekblad/codelight) (MIT, Python) | 6 agents, widget et notification Android, approbation | Petit projet, hors réseau local non documenté |
| [CodeIsland](https://github.com/wxtsky/CodeIsland) (MIT) | 30+ outils, encoche Mac, iPhone, ESP32 | Côté Apple ; Android par ntfy seulement |
| [Happy](https://github.com/slopus/happy) (MIT) | App Android native, relais chiffré | Client de session complet, pas une brique |
| [zedra](https://github.com/deltaqdev/zedra), [cawide](https://github.com/Hermillon/cawide), [ciao-host](https://github.com/bojanstef/ciao-host) | Démon Rust + hooks + iroh + QR | Applications de télécommande ; à lire pour l'installation des hooks, l'appairage et uniffi |
| [layon](https://github.com/up9cloud/layon) (MIT) | Même architecture cible (binaire Rust, iroh, QR, uniffi) | Veille/réveil de machine seulement, agents « prévus » |
| Vibe Island (fermé, 19,99 $) | Encoche Mac, 30 agents | Mac seul |
| Remote Control (Anthropic), contrôle à distance Copilot, app ChatGPT pour Codex | Suivi et réponse officiels | Un seul éditeur chacun, pas d'API tierce |

Vide constaté : aucune brique ne réunit binaire unique, plusieurs agents, états standardisés, réseau sans compte et bibliothèque cliente pour écrans tiers.

## Standards
- **A2A 1.0.1** ([spec](https://a2a-protocol.org/latest/specification/)) : `TaskState` = submitted, working, input-required, auth-required, completed, failed, canceled, rejected. Flux SSE, webhooks, observateurs multiples prévus. Gouvernance Linux Foundation. Les agents de code de bureau ne le parlent pas nativement. SDK Rust officiel : [a2a-rs](https://github.com/a2aproject/a2a-rs) (0.x).
- **MCP Tasks** (extension) : working, input_required, completed, failed, cancelled — niveau outil, pas agent.
- **ACP (Zed)** : protocole un-à-un entre éditeur et agent ; pas d'observateur tiers.
- **AG-UI** : événements fins d'une interface vers son agent ; pas de modèle d'observateur.
- **Protocole Bluetooth « Hardware Buddy »** ([claude-desktop-buddy](https://github.com/anthropics/claude-desktop-buddy)) : seul format d'objet repris par des tiers ; compteurs agrégés, émis par l'app Claude de bureau.

## Hooks des agents
- **Claude Code** ([doc](https://code.claude.com/docs/en/hooks)) : hooks `command`, `http`, `mcp_tool`, `prompt`, `agent`. Champs communs `session_id`, `cwd`, `transcript_path`, `hook_event_name`. Utiles : `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PermissionRequest` (réponse `allow`/`deny`, bloquant jusqu'à 600 s par défaut), `Notification` (`permission_prompt`, `idle_prompt`), `Stop`, `StopFailure`, `SessionEnd`. Les hooks s'appliquent aussi aux sous-agents.
- **Codex** ([doc](https://developers.openai.com/codex/hooks)) : format repris de Claude Code, dans `~/.codex/hooks.json` ou la table `[hooks]` de `config.toml` ; types `command` et `mcp_tool` (pas de `http`). Les noms ne sont pas tous les mêmes : `Interrupt`, `PreModelRequest` et `PostModelRequest` en plus ; ni `Notification`, ni `StopFailure`, ni `PostToolUseFailure`. Chaque événement porte `model`. Un hook nouveau ou modifié ne tourne qu'après approbation par l'utilisateur. Corrigé le 2026-11-22 d'après la doc et le dépôt openai/codex 0.171.0 (la version du 2026-11-17 disait « mêmes noms et mêmes champs ») : voir DECISIONS D16.
- **Copilot CLI** ([doc](https://docs.github.com/en/copilot/reference/hooks-configuration)) : ses propres hooks dans `~/.copilot/hooks/*.json`, types `command` et `http` (`http://127.0.0.1` seulement avec `COPILOT_HOOK_ALLOW_LOCALHOST=1`), noms en camelCase (`agentStop`, `permissionRequest`, `errorOccurred`…), pas de champ modèle. Ne lit `.claude/settings.json` que dans le dépôt, pas dans le dossier de l'utilisateur. Relevé du 2026-11-22 (la version du 2026-11-17 disait « lit directement `.claude/settings.json` »), à revérifier avant de coder.
- **Cursor** ([doc](https://cursor.com/docs/agent/hooks)) : ses propres hooks dans `~/.cursor/hooks.json`, type `command` seulement (pas de `http`) ; charge aussi les hooks Claude, sauf `Notification` et `PermissionRequest` : aucun événement ne dit qu'il attend l'utilisateur. Relevé du 2026-11-22, à revérifier avant de coder.
- **OpenCode** (dépôt passé à `anomalyco/opencode`) : API HTTP et flux SSE plutôt que des hooks ; extensions en JavaScript, deux lignes de versions (1.x et 2.x) aux interfaces différentes. Relevé du 2026-11-22, à revérifier avant de coder.
- Socle portable : `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `Stop`, `SessionEnd` pour Claude Code et Codex ; Copilot CLI et Cursor ont les mêmes étapes sous d'autres noms.

## Réseau
- **iroh 1.4.1** (1.0 le 2026-06-15, MIT/Apache) : Linux, macOS, Windows, Android, iOS ; liaisons Kotlin et Swift (socle QUIC seulement) ; navigateur par relais ; ESP32 hors réseau local seulement avec PSRAM.
- Limites : ne réveille pas un téléphone ; relais publics réservés au développement ; battement de 5 s non réglable (#4651) ; retombées sur relais derrière certains NAT d'opérateurs.

## Coût d'une app Android toujours éveillée (mesure)
Launcher Encre sur Galaxy A36, 13 h 13 sur batterie : 10,8 mAh de processeur (launcher Samsung : 9,7 ; Oasis : 3,9), soit environ 0,4 % de batterie par jour. Rester en mémoire ne coûte presque rien ; ce sont les réveils et la radio qui coûtent.
