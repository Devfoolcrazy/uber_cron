# PLAN.md — UberCron (éditeur graphique cron & launchd pour macOS)

> Ce document est la source de vérité du projet. Toute déviation en cours d'implémentation
> doit être consignée dans `DECISIONS.md` (créer le fichier au premier écart) avec la date,
> la décision, et la raison.

## 1. Vision

Application desktop macOS (Rust + Tauri 2) permettant de **visualiser, créer, modifier,
supprimer et activer/désactiver** des tâches planifiées, sur deux backends au choix de
l'utilisateur :

- **cron** : la crontab utilisateur (`crontab -l` / `crontab -`)
- **launchd** : les LaunchAgents utilisateur (`~/Library/LaunchAgents/*.plist`)

L'utilisateur choisit le backend dans l'UI, voit la liste des jobs du backend courant, et
effectue le CRUD dans un formulaire adapté au backend. Pas de démon interne : l'app ne
fait qu'éditer/piloter les schedulers natifs du système.

**Hors scope (MVP)** : Windows, Linux, LaunchDaemons système (`/Library/LaunchDaemons`),
crontabs d'autres utilisateurs, exécution *planifiée* de jobs par l'app elle-même (le
"Run now" à la demande est, lui, dans le MVP — voir §5.5 et §6.6).

**Distribution** : open-source, build soi-même. Pas de binaire distribué ni de
notarisation ; signature ad hoc suffisante. UI bilingue **FR + EN** dès le MVP
(react-i18next, défaut = langue système).

## 2. Stack technique

| Couche | Choix | Notes |
|---|---|---|
| Backend | Rust (edition 2021+) | Toute la logique métier vit côté Rust |
| Shell app | Tauri 2 | Commands IPC, pas de plugin exotique au MVP |
| Frontend | React + Vite + TypeScript | |
| Parsing cron | crate `croner` (ou `cron`) | Calcul des prochaines occurrences uniquement — la phrase humaine est générée côté frontend |
| Parsing plist | crate `plist` | Lecture/écriture XML plist |
| Erreurs | `thiserror` (lib) + `anyhow` interdit dans la lib métier | Erreurs typées exposées à l'IPC |
| Sérialisation IPC | `serde` + `tauri-specta` v2 | Types TS et wrappers `invoke()` **générés** depuis les structs Rust — jamais de miroir manuel |
| Humanisation schedule | `cronstrue` (frontend) | Locales FR/EN incluses ; launchd humanisé par une fonction TS dédiée |
| i18n | `react-i18next` | FR + EN dès le MVP |
| Tests | `cargo test` + golden files ; `vitest` pour l'humaniseur launchd TS | Voir §9 |

Structure du repo :

```
/src-tauri
  /src
    /backend
      mod.rs          # trait SchedulerBackend + types communs
      cron/           # CronBackend : parser crontab, writer, mapping
      launchd/        # LaunchdBackend : plist, launchctl
    /model            # Job, JobSpec, ScheduleSpec, JobStatus...
    /commands.rs      # commands Tauri (couche fine, aucune logique)
    main.rs
/src                  # frontend React
  /components
  /views              # JobList, JobEditorCron, JobEditorLaunchd, Diagnostics
  /lib/bindings.ts    # GÉNÉRÉ par tauri-specta — ne jamais éditer à la main
  /lib/schedule.ts    # humanisation : cronstrue (cron) + fonction TS (launchd)
  /locales            # fr.json, en.json
PLAN.md
DECISIONS.md          # créé au premier écart
```

## 3. Modèle de données

### 3.1 Modèle commun (affichage / liste)

```rust
enum BackendKind { Cron, Launchd }

struct Job {
    id: JobId,                    // voir §3.2
    backend: BackendKind,
    label: String,                // launchd: Label ; cron: dérivé (commentaire # name: ... ou tronqué de la commande)
    command: String,              // commande affichable
    schedule: ScheduleInfo,       // structuré — le frontend en dérive la phrase humaine (cronstrue / fn TS)
    schedule_raw: String,         // "0 9 * * 1" ou résumé du StartCalendarInterval
    enabled: bool,
    next_runs: Vec<DateTime>,     // 5 prochaines occurrences (peut être vide si non calculable)
    status: JobStatus,            // launchd: Running/Loaded/NotLoaded ; cron: toujours Static
    last_exit_code: Option<i32>,  // launchd: extrait de `launchctl print` (défensif) ; cron: None
    managed: bool,                // launchd: label préfixé com.ubercron. ; cron: toujours true
}

enum ScheduleInfo {
    CronExpr(String),                       // 5 champs ou @raccourci (@daily, @reboot...)
    CalendarIntervals(Vec<CalendarEntry>),
    Interval(u64),                          // secondes
}
```

La phrase humaine ("Tous les lundis à 09:00") n'est PAS générée côté Rust : le frontend
la produit depuis `ScheduleInfo` (cronstrue avec locale FR/EN pour cron, fonction TS
testée pour launchd). Rust ne fournit que les données structurées et les `next_runs`.

### 3.2 Identité des jobs

- **launchd** : `JobId = Label` du plist (unique par fichier). Le nom de fichier est
  `{label}.plist`.
- **cron** : pas d'identité native. On identifie une ligne par **index de ligne dans la
  crontab au moment du snapshot** + hash du contenu de la ligne. Toute opération d'écriture
  relit la crontab, vérifie que le hash correspond toujours (détection d'édition
  concurrente), sinon renvoie une erreur `ConcurrentModification` que l'UI affiche avec
  proposition de recharger.
- **Contrat d'invalidation (cron)** : toute mutation réussie (create/update/delete/enable)
  invalide TOUS les `JobId` cron détenus par le frontend (les index bougent). L'UI doit
  re-fetch la liste complète après chaque mutation — sinon la deuxième action d'affilée
  déclenche un faux `ConcurrentModification` causé par l'app elle-même.

### 3.3 Spécification d'édition (par backend, PAS de formulaire unifié)

```rust
enum JobSpec {
    Cron(CronJobSpec),        // expression 5 champs + commande + (option) nom via commentaire
    Launchd(LaunchdJobSpec),  // label, ProgramArguments, schedule, logs, RunAtLoad...
}

enum LaunchdSchedule {
    CalendarIntervals(Vec<CalendarEntry>),  // StartCalendarInterval (dict ou array de dicts)
    Interval(u64),                          // StartInterval (secondes)
}
```

**Décision d'architecture ferme** : on ne tente PAS de compiler un formulaire commun vers
les deux backends. Les cas non traduisibles (`*/5` cron ↔ array d'entrées calendar launchd,
`StartInterval` sans équivalent cron) rendent l'unification piégeuse. Un modèle commun en
lecture, deux formulaires en écriture.

## 4. Trait SchedulerBackend

```rust
trait SchedulerBackend {
    fn kind(&self) -> BackendKind;
    fn list(&self) -> Result<Vec<Job>, BackendError>;
    fn get(&self, id: &JobId) -> Result<Job, BackendError>;
    fn create(&self, spec: &JobSpec) -> Result<JobId, BackendError>;
    fn update(&self, id: &JobId, spec: &JobSpec) -> Result<(), BackendError>;
    fn delete(&self, id: &JobId) -> Result<(), BackendError>;
    fn set_enabled(&self, id: &JobId, enabled: bool) -> Result<(), BackendError>;
    fn run_now(&self, id: &JobId) -> Result<RunResult, BackendError>;   // voir §5.5 / §6.6
    fn diagnostics(&self) -> Vec<Diagnostic>;   // voir §8
}

enum RunResult {
    Completed { exit_code: i32, stdout_tail: String, stderr_tail: String },  // cron : exécution directe
    Started,   // launchd : kickstart est asynchrone — consulter le statut / les logs
}
```

`BackendError` : enum typée (`NotFound`, `ConcurrentModification`, `ParseError { line }`,
`CommandFailed { cmd, stderr }`, `PermissionDenied`, ...). Les commands Tauri la
convertissent en une structure sérialisable `{ code, message, detail }`.

## 5. Backend cron

### 5.1 Lecture

- `crontab -l` via `std::process::Command`. Exit code 1 + stderr "no crontab for" ⇒ crontab
  vide, pas une erreur.
- **Parsing lossless** : la crontab est représentée comme une liste ordonnée de lignes typées :

```rust
enum CrontabLine {
    Job { schedule: String, command: String, name: Option<String>, raw: String },
    DisabledJob { schedule: String, command: String, name: Option<String>, raw: String },  // ligne "# UBERCRON-DISABLED: ..." — mêmes champs que Job (pas de Box<CrontabLine> : on ne "désactive" pas un commentaire)
    EnvVar { key: String, value: String, raw: String },     // SHELL=, PATH=, MAILTO=...
    Comment { raw: String },
    Blank,
    Unknown { raw: String },                                // ligne non parsable : PRÉSERVÉE telle quelle
}
```

- **@-raccourcis** : le cron de macOS (Vixie) accepte `@reboot`, `@daily`, `@hourly`,
  `@weekly`, `@monthly`, `@yearly`. Ces lignes sont parsées comme des `Job` à part entière
  (schedule = le raccourci), PAS comme `Unknown` — sinon des jobs réels deviennent
  invisibles dans la liste. L'éditeur du MVP ne propose pas de créer des @-raccourcis
  (les presets génèrent du 5 champs), mais il les affiche et les préserve.
- **Invariant round-trip** : `parse(text) → serialize() == text` pour toute crontab non
  modifiée. Test golden obligatoire (§9). On ne reformate JAMAIS une ligne qu'on n'a pas
  éditée.
- Nommage optionnel des jobs : un commentaire `# name: Mon backup` sur la ligne précédant
  immédiatement un job est interprété comme son nom (et réécrit avec lui).

### 5.2 Écriture

- Reconstruction du texte complet puis `echo "$content" | crontab -` (en Rust : spawn
  `crontab -` avec stdin pipé). Jamais de `crontab fichier` (écrase sans passer par le
  spool proprement sur certaines configs) ni d'édition de `/var/at/tabs` en direct.
- Avant chaque écriture : relire `crontab -l`, vérifier le hash global du snapshot (§3.2).
- **Backup automatique** : avant toute écriture, copie horodatée du contenu courant dans
  `~/Library/Application Support/UberCron/backups/crontab-{timestamp}.txt`. Garder les 20
  dernières. Un menu "Restaurer un backup" est un stretch goal, mais les fichiers doivent
  exister dès le MVP.

### 5.3 Enable/disable

Convention : préfixer la ligne par `# UBERCRON-DISABLED: `. Au parsing, une ligne commençant
par ce marqueur est un `DisabledJob` (on parse ce qui suit le marqueur comme une ligne job
normale). Les jobs commentés "à la main" par l'utilisateur restent des `Comment` (on ne
devine pas).

### 5.4 Prochaines occurrences

Via `croner` sur l'expression 5 champs, timezone locale, 5 prochaines dates.
Les @-raccourcis sont convertis en équivalent 5 champs pour le calcul (`@daily` → `0 0 * * *`,
etc.), sauf `@reboot` : `next_runs` vide + badge "au démarrage" dans l'UI.

### 5.5 Run now (cron)

Pas d'équivalent natif côté cron : "Exécuter maintenant" lance la commande du job via
`/bin/sh -c` (même sémantique que le wrapper cron), en capturant exit code + queues de
stdout/stderr (`RunResult::Completed`). Exécution asynchrone côté UI (spinner + bouton
Annuler qui tue le process). Ne modifie jamais la crontab. Avertissement affiché : cron
exécute avec un PATH minimal — le run-now hérite de l'environnement de l'app, le résultat
peut donc différer d'une exécution planifiée réelle.

## 6. Backend launchd

### 6.1 Périmètre

Uniquement `~/Library/LaunchAgents/`. On liste les `.plist` du dossier. Les agents chargés
depuis d'autres emplacements sont hors scope MVP.

**Agents gérés vs externes** : ce dossier contient chez la plupart des utilisateurs des
dizaines de plists déposés par des apps tierces (Adobe, Google, homebrew services...).
Un job est "géré" si son label commence par `com.ubercron.` (champ `managed` de §3.1).
Tout est listé, mais : badge visuel distinct, filtre rapide géré/externe dans la liste,
et l'édition/suppression d'un agent externe exige une confirmation avec avertissement
explicite ("cet agent appartient probablement à une autre application"). Un plist externe
contenant des clés que le formulaire ne représente pas (MachServices, WatchPaths...) reste
éditable grâce à la préservation §6.3, mais l'avertissement le mentionne.

### 6.2 Source de vérité

Les **fichiers plist** (crate `plist`, format XML). `launchctl` sert uniquement à :
- charger/décharger : `launchctl bootstrap gui/$UID {path}` / `launchctl bootout gui/$UID/{label}`
- statut runtime : `launchctl print gui/$UID/{label}` — parsing minimal et défensif
  (format non garanti par Apple) : on extrait seulement `state = running` et `pid`. Si le
  parsing échoue, statut = `Unknown`, pas d'erreur bloquante.

`$UID` obtenu via `libc::getuid()` ou `id -u`.

### 6.3 Clés plist gérées par l'éditeur (MVP)

`Label`, `ProgramArguments` (array — l'UI propose un champ commande simple qui est splitté
en argv OU un mode "wrapper shell" `["/bin/sh", "-c", cmd]`, au choix de l'utilisateur ;
défaut : wrapper shell, plus proche du comportement cron), `StartCalendarInterval` (dict ou
array de dicts : Minute, Hour, Day, Weekday, Month), `StartInterval`, `RunAtLoad`,
`StandardOutPath`, `StandardErrorPath`, `Disabled`.

**Préservation** : un plist peut contenir des clés qu'on ne gère pas (`KeepAlive`,
`EnvironmentVariables`, ...). On les préserve intégralement au round-trip : lecture en
`plist::Value` (dictionnaire générique), modification des seules clés éditées,
réécriture. Jamais de désérialisation vers une struct fermée.

### 6.4 CRUD

- **create** : vérifier l'absence de collision (fichier `{label}.plist` existant OU service
  déjà présent dans le domaine gui) → sinon erreur. Écrire le plist puis `bootstrap`.
  Label suggéré : `com.ubercron.{slug}`.
- **update** : si le job est **running**, confirmation UI obligatoire ("le process en cours
  sera tué"). Backup du plist courant (voir ci-dessous), puis `bootout` → réécrire →
  `bootstrap`. Si le job n'était pas chargé, écrire seulement.
- **delete** : confirmation UI obligatoire (renforcée si agent externe, §6.1). `bootout`
  (ignorer l'erreur "not loaded") puis **déplacement** du plist vers
  `~/Library/Application Support/UberCron/trash/` — jamais de suppression définitive.
- **Backups plist** : avant chaque update, copie horodatée dans
  `~/Library/Application Support/UberCron/backups/launchd/{label}-{timestamp}.plist`
  (garder les 20 dernières par label). Symétrique des backups crontab §5.2.

### 6.5 Enable/disable — trois sources d'état

launchd a TROIS sources d'état d'activation, pas deux :

1. la clé `Disabled` du plist (héritage, consultée au login) ;
2. la **base d'overrides** pilotée par `launchctl enable/disable gui/$UID/{label}`
   (persistée dans `/var/db/com.apple.xpc.launchd/disabled.{uid}.plist`) — elle **prime
   sur la clé `Disabled`** et fait échouer `bootstrap` si le service y est désactivé ;
3. l'état chargé/déchargé courant du domaine gui.

Procédure retenue :
- **disable** : `launchctl bootout gui/$UID/{label}` + `launchctl disable gui/$UID/{label}`.
- **enable** : `launchctl enable gui/$UID/{label}` PUIS `bootstrap` (sans le `enable`
  préalable, le bootstrap d'un service override-disabled échoue et le job devient
  irrécupérable depuis l'app).
- On n'écrit JAMAIS la clé `Disabled` dans le plist (primée par l'override DB) ; on la lit
  seulement pour l'affichage.
- L'état `enabled` affiché combine défensivement les trois sources : lecture de
  `launchctl print-disabled gui/$UID` + clé `Disabled` + présence dans le domaine.

**Spike obligatoire avant d'implémenter (étape 7 du §10)** : valider ce comportement en
terminal sur un agent jetable et consigner les sorties réelles dans `DECISIONS.md`.

### 6.6 Run now & dernier statut (launchd)

- **Run now** : `launchctl kickstart gui/$UID/{label}` (`RunResult::Started` — asynchrone,
  pas de capture de sortie ; l'UI pointe vers la visionneuse de logs). Si le job n'est pas
  chargé, proposer de l'activer d'abord.
- **Dernier statut** : `last exit code` extrait de `launchctl print gui/$UID/{label}` avec
  le même parsing défensif que le statut (§6.2) — si introuvable, `None`, jamais bloquant.
  Affiché dans la liste (badge vert/rouge).

### 6.7 Prochaines occurrences

Calculées en Rust depuis `StartCalendarInterval` (itération sur les 7 prochains jours,
suffisant pour 5 occurrences dans la quasi-totalité des cas ; si aucune trouvée sur 366
jours, afficher "—"). Pour `StartInterval` : approximation "toutes les N secondes" sans
dates absolues (afficher la période, pas des timestamps).

## 7. Surface IPC (commands Tauri)

Couche fine, zéro logique métier :

```
list_jobs(backend: BackendKind) -> Vec<Job>
get_job(backend, id) -> Job
create_job(backend, spec: JobSpec) -> JobId
update_job(backend, id, spec) -> ()
delete_job(backend, id) -> ()
set_job_enabled(backend, id, enabled: bool) -> ()
run_job(backend, id) -> RunResult
preview_schedule(schedule: ScheduleInfo) -> Vec<String>  // 5 prochaines occurrences pour l'aperçu live ; la phrase humaine est calculée côté frontend
run_diagnostics(backend) -> Vec<Diagnostic>
```

Types TS et wrappers `invoke()` **générés par `tauri-specta`** dans `src/lib/bindings.ts`
(fichier généré, jamais édité à la main). Aucun miroir manuel : le drift Rust/TS est
structurellement impossible.

## 8. UI

### 8.1 Écrans

1. **Sélecteur de backend** : deux onglets/segments en haut ("cron" / "launchd"),
   persistés entre sessions (store Tauri ou localStorage).
2. **Liste des jobs** (par backend) : nom, schedule humain (calculé côté front, §3.1),
   commande (tronquée), badge état (actif/désactivé, + running pour launchd), badge
   géré/externe (launchd, §6.1) avec filtre rapide, dernier statut d'exécution (launchd :
   badge vert/rouge sur le last exit code), prochaine exécution.
   Actions par ligne : exécuter maintenant, éditer, activer/désactiver, supprimer (avec
   confirmation). Le run-now cron ouvre un panneau de résultat (exit code + sortie).
3. **Formulaire cron** : builder 5 champs (presets : toutes les minutes / heures / jours /
   hebdo / mensuel / expression libre), champ commande, champ nom optionnel.
   **Aperçu live** : phrase humaine (cronstrue, locale courante) + 5 prochaines exécutions
   via `preview_schedule`, mis à jour à la frappe.
4. **Formulaire launchd** : label, commande (mode shell wrapper par défaut), type de
   schedule (calendar avec builder d'entrées / interval en secondes), RunAtLoad,
   chemins de logs (avec valeurs par défaut proposées dans
   `~/Library/Logs/UberCron/{label}.out.log` / `.err.log`). Même aperçu live.
5. **Diagnostics** : accessible depuis un bouton d'entête. Affiche les checks du backend
   courant :
   - cron : `crontab` accessible ? Warning Full Disk Access (lien texte expliquant que
     `/usr/sbin/cron` doit être ajouté dans Réglages > Confidentialité et sécurité >
     Accès complet au disque si des jobs touchent Documents/Desktop/Downloads). Warning
     PATH minimal de cron ("utilisez des chemins absolus dans vos commandes").
   - launchd : dossier LaunchAgents accessible ? `launchctl` répond ?
6. **Visionneuse de logs** (launchd seulement, MVP simple) : si le job a
   `StandardOutPath`/`StandardErrorPath`, bouton "voir les logs" → affiche les N dernières
   lignes du fichier (tail simple côté Rust, pas de suivi live au MVP).

### 8.2 i18n

Toutes les chaînes UI passent par react-i18next (`fr.json` / `en.json`), langue par défaut
= langue système, commutable dans l'app. Les phrases de schedule suivent la locale
(cronstrue supporte fr et en). Les messages d'erreur Rust exposent un `code` stable ;
c'est le frontend qui les traduit (le `message` Rust ne sert que de fallback technique).

### 8.3 Hors MVP (backlog UI)

- Timeline fusionnée 24h tous backends confondus
- Restauration de backup crontab / corbeille plist depuis l'UI (les fichiers existent dès le MVP)
- Suivi live des logs
- Recherche/filtre texte dans la liste
- Historique des run-now cron

## 9. Tests

- **Parser crontab** : suite de golden files (`tests/fixtures/crontabs/*.txt`) couvrant :
  crontab vide, commentaires, variables d'env, lignes invalides, jobs nommés, jobs
  désactivés, @-raccourcis (dont `@reboot`), mélanges. Invariant round-trip
  parse→serialize == identité sur chaque fixture.
- **Parser/writer plist** : fixtures de plists réels (dont un avec clés non gérées type
  `KeepAlive`) — vérifier la préservation des clés inconnues au round-trip.
- **Calcul d'occurrences** : cas cron classiques (`*/5`, `0 9 * * 1`, `0 0 1 * *`) et
  calendar launchd (Weekday, array d'entrées), comparés à des valeurs attendues fixes
  (timezone figée dans les tests).
- **Détection ConcurrentModification** : test unitaire sur le mécanisme de hash.
- **Matrice enable/disable launchd** : via le mock `SystemCommands`, couvrir les
  combinaisons des trois sources d'état (§6.5) — notamment "override-disabled + bootstrap
  doit être précédé d'un enable" et "clé Disabled présente mais override absent".
- **Humaniseur launchd TS** : tests vitest sur `src/lib/schedule.ts` (entrées calendar
  simples, array d'entrées, Interval), en fr et en.
- Les appels système (`crontab`, `launchctl`) sont derrière un trait `SystemCommands`
  mocké dans les tests ; aucune écriture réelle de crontab/plist dans `cargo test`.

## 10. Étapes d'implémentation

Chaque étape doit compiler, passer les tests, et être committée avant la suivante.

1. **Squelette** : init Tauri 2 + React + Vite + TS, intégration `tauri-specta` et
   squelette i18n (react-i18next, fr+en), structure de dossiers §2, CI locale
   (`cargo test` + `cargo clippy -- -D warnings` + `vitest`).
2. **Modèle + trait** : types §3, trait §4 (dont `run_now`/`RunResult`), `BackendError`,
   trait `SystemCommands` + mock.
3. **Parser crontab lossless** (incluant @-raccourcis) + tests golden (round-trip).
   Aucune écriture encore.
4. **CronBackend complet** : list/create/update/delete/enable/run_now via mock, puis
   branchement sur le vrai `crontab`. Backups §5.2.
5. **Occurrences + preview** : intégration `croner` (+ conversion des @-raccourcis),
   command `preview_schedule` (next_runs uniquement).
6. **UI cron** : liste + formulaire + aperçu live (cronstrue) + run-now avec panneau de
   résultat + diagnostics cron, le tout bilingue.
   → **JALON BLOQUANT** : *l'app est utilisable en cron-only. L'utiliser en conditions
   réelles plusieurs jours ; consigner les retours dans `DECISIONS.md`. On ne commence
   launchd qu'après ce retour d'usage.*
7. **Spike launchd** (une demi-journée, en terminal, aucun code) : valider sur un agent
   jetable le comportement réel de l'override DB (`enable`/`disable`/`print-disabled`/
   échec de `bootstrap` sur service désactivé), `kickstart`, et le format de
   `launchctl print` (statut + last exit code) sur la version d'OS cible. Consigner les
   sorties brutes dans `DECISIONS.md` — elles servent de fixtures aux mocks.
8. **Lecture launchd** : scan LaunchAgents, parsing plist générique, statut + last exit
   code via `launchctl print` (défensif), badge géré/externe, affichage liste.
9. **CRUD launchd** : create/update/delete/enable selon §6.4–6.5 (backups, corbeille,
   garde-fou job running, procédure enable via override DB), occurrences calendar,
   kickstart.
10. **UI launchd** : formulaire, run-now, logs viewer, avertissements agents externes,
    diagnostics launchd.
11. **Finitions** : persistance du backend sélectionné, confirmations, états d'erreur UI
    (dont ConcurrentModification avec bouton recharger), icône, build `.app` signé ad hoc,
    README (build soi-même, prérequis, captures).

## 11. Risques & pièges connus (à garder en tête pendant l'implémentation)

- **PATH minimal de cron** : les commandes utilisateur échouent souvent pour ça. Le
  diagnostic + un warning dans le formulaire si la commande ne commence pas par `/` ou
  `~` couvrent le MVP.
- **Full Disk Access** : ni l'app ni cron ne peuvent lire Documents/Desktop sans TCC.
  L'app elle-même n'a besoin que de `~/Library/LaunchAgents` (pas protégé), OK.
- **`launchctl print` non stable** : parsing défensif, jamais bloquant.
- **Override DB launchd** (§6.5) : `bootstrap` échoue sur un service désactivé via
  `launchctl disable` — toujours `enable` avant `bootstrap`, jamais se fier à la seule
  clé `Disabled` du plist.
- **`bootout` tue le process** : jamais d'update/delete silencieux sur un job running
  (garde-fou §6.4).
- **`Weekday` launchd** : 0 ET 7 signifient dimanche — normaliser au parsing.
- **Agents tiers dans LaunchAgents** : la liste launchd est bruyante par défaut ; le badge
  géré/externe + filtre (§6.1) sont indispensables, pas cosmétiques.
- **Éditions concurrentes** (crontab -e dans un terminal pendant que l'app tourne) :
  couvert par le hash §3.2 ; la liste doit avoir un bouton "Recharger". Côté frontend,
  re-fetch systématique après chaque mutation cron (§3.2).
- **Ne jamais toucher** : `/Library/LaunchDaemons`, `/Library/LaunchAgents`,
  `/System/Library/...`, crontab root.

## 12. Décisions de cadrage (2026-07-09)

Arrêtées avant la première ligne de code, après revue critique du plan :

1. **Distribution** : open-source build-soi-même — pas de notarisation, signature ad hoc.
2. **Séquencement** : jalon cron-only (étapes 1–6) livré et utilisé en réel AVANT tout
   code launchd ; spike terminal launchd obligatoire (étape 7).
3. **Run now + dernier statut** : au MVP pour les deux backends (kickstart/last exit code
   côté launchd ; exécution `/bin/sh -c` capturée côté cron).
4. **Agents tiers launchd** : tout listé, badge géré (`com.ubercron.*`) vs externe,
   garde-fous à l'édition/suppression des externes.
5. **i18n** : FR + EN dès le MVP (react-i18next), défaut = langue système.
6. **Humanisation des schedules** : côté frontend (cronstrue + fonction TS launchd) ;
   Rust ne fournit que données structurées et occurrences.
7. **Bindings Rust ↔ TS** : générés par tauri-specta, aucun miroir manuel.
