# tallylight

Brique open source (Rust) qui relie n'importe quel agent IA de code à n'importe quel écran : un binaire par machine lit l'état des sessions (au travail, attend une réponse, fini), le traduit dans un vocabulaire standard et le diffuse aux appareils appairés, sans compte ni serveur. Dépôt public depuis le 2026-11-22 (version 0.1.0, site sur GitHub Pages) : tout ce qui est poussé est visible.

Docs à lire au démarrage :
- `docs/ARCHITECTURE.md` — composants, flux, périmètre de la première version
- `docs/DECISIONS.md` — choix et raisons (prioritaire : tout choix non évident y est tracé)
- `docs/INFRA.md` — compilation, machines, tests sur appareil
- `docs/PROTOCOL.md` — le protocole entre machine et écran, en anglais (contrat public)
- `docs/ETAT-DE-L-ART.md` — étude de l'existant du 2026-11-17 (concurrents, standards, hooks, réseau)

Règles :
- Le produit est la bibliothèque, pas une application : ne pas dériver vers une télécommande d'agents.
- Variables et noms en anglais, commentaires en français. Tout ce que voit l'utilisateur (messages du binaire, README, site) est en anglais (décision D14).
- Mettre à jour la doc dans le même commit que le changement.
