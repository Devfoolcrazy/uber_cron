<p align="center">
  <img src="art/icon.svg" width="128" alt="Icône UberCron" />
</p>

# UberCron

Éditeur graphique de tâches planifiées pour macOS : **cron** (crontab utilisateur) et
**launchd** (LaunchAgents), dans une seule app native.

> **English TL;DR** — A macOS desktop app (Rust + Tauri 2) to view, create, edit,
> enable/disable and test your cron jobs and launchd agents. Bilingual UI (FR/EN).
> Build it yourself: `npm install && npm run tauri build`. Docs are in French.

## Fonctionnalités

- **Liste unifiée** : nom, phrase humaine (« Tous les lundis à 09:00 »), expression
  exacte, prochaine exécution, état, dernier code de sortie (launchd).
- **Éditeur cron guidé** : builder à modes (chaque / toutes les N / valeur précise /
  personnalisé), presets, aide intégrée avec exemples cliquables, aperçu live
  (phrase + 5 prochaines dates), commande structurée (dossier de travail, journal)
  avec conseils anti-pièges (PATH minimal, chemins absolus, journal manquant).
- **launchd complet** : agents gérés vs externes (badge + filtre), calendrier
  multi-horaires ou intervalle, RunAtLoad, visionneuse de journaux, gestion
  correcte de l'override DB (`launchctl enable/disable`).
- **Exécuter maintenant** : test immédiat d'une tâche (sortie capturée côté cron,
  `kickstart` côté launchd).
- **Sécurité des données** : parsing *lossless* de la crontab (vos commentaires et
  votre mise en forme sont préservés à l'octet près), détection des éditions
  concurrentes, backups horodatés avant chaque écriture, suppression launchd en
  corbeille (jamais de `rm`), préservation des clés plist inconnues.

L'app n'exécute rien elle-même : elle pilote les schedulers natifs du système.
Fermez-la, vos tâches continuent.

## Compiler soi-même

Prérequis : macOS 12+, [Rust](https://rustup.rs) ≥ 1.91, Node ≥ 20, Xcode Command
Line Tools.

```sh
git clone <ce repo> && cd uber_cron
npm install
npm run tauri build   # produit UberCron.app (+ .dmg) dans src-tauri/target/release/bundle/
```

Développement : `npm run tauri dev`. CI locale : `scripts/ci.sh`
(clippy `-D warnings`, tests Rust, tsc, vitest).

La signature est *ad hoc* : au premier lancement d'un binaire que vous n'avez pas
compilé vous-même, macOS demandera clic droit → Ouvrir.

## Bon à savoir (cron sur macOS)

- **Full Disk Access** : si vos tâches cron lisent Documents/Bureau/Téléchargements,
  ajoutez `/usr/sbin/cron` dans Réglages Système → Confidentialité et sécurité →
  Accès complet au disque. L'écran Diagnostics de l'app le rappelle.
- **PATH minimal** : cron exécute avec `/usr/bin:/bin`. Utilisez des chemins
  absolus — l'éditeur vous prévient et vous guide.

## Architecture

- `PLAN.md` — la source de vérité du projet (modèle, invariants, décisions).
- `DECISIONS.md` — les écarts au plan, datés et motivés (dont les sorties brutes
  du spike launchd qui servent de fixtures aux tests).
- Backend Rust (`src-tauri/`) : toute la logique ; les appels système passent par
  un trait mocké dans les tests — `cargo test` n'écrit jamais dans votre crontab.
- Frontend React (`src/`) : bindings TypeScript générés par tauri-specta
  (`src/lib/bindings.ts`, ne jamais éditer à la main).

## Licence

À définir.
