# tallylight

Brique open source (Rust) qui relie n'importe quel agent IA de code à n'importe quel écran : un binaire par machine lit l'état des sessions (au travail, attend une réponse, fini), le traduit dans un vocabulaire standard et le diffuse aux appareils appairés, sans compte ni serveur. Dépôt privé jusqu'à la première démo.

Docs à lire au démarrage :
- `docs/ARCHITECTURE.md` — composants, flux, périmètre de la première version
- `docs/DECISIONS.md` — choix et raisons (prioritaire : tout choix non évident y est tracé)
- `docs/INFRA.md` — compilation, machines, tests sur appareil
- `docs/ETAT-DE-L-ART.md` — étude de l'existant du 2026-11-17 (concurrents, standards, hooks, réseau)

Règles :
- Le produit est la bibliothèque, pas une application : ne pas dériver vers une télécommande d'agents.
- Variables et noms en anglais, commentaires en français.
- Mettre à jour la doc dans le même commit que le changement.
