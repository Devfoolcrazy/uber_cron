# DECISIONS.md — écarts par rapport à PLAN.md

> Créé au premier écart, comme prévu par le plan.

## 2026-07-10 — Spike launchd (étape 7) : sorties brutes observées

Agent jetable `com.ubercron.spike` (StartInterval 86400, `exit 3`), Darwin 25.5.0.
Ces sorties servent de fixtures aux mocks des tests.

- `launchctl print gui/$UID/<label>` (service chargé, exit 0) — extraits :
  `state = not running` puis après kickstart `last exit code = 3` ;
  avant première exécution : `last exit code = (never exited)`.
  **Piège** : des lignes `state = active` apparaissent aussi dans des
  sous-sections plus indentées → ne parser que la PREMIÈRE occurrence.
- `launchctl print` sur service NON chargé : exit **113**,
  stderr `Could not find service "…" in domain for user gui: 501`.
  → test de chargement fiable : exit 0 vs 113.
- `launchctl disable gui/$UID/<label>` puis `bootstrap` :
  **`Bootstrap failed: 5: Input/output error`** (exit 5) — erreur trompeuse,
  aucune mention de désactivation. Confirme §6.5 : toujours `enable` avant
  `bootstrap`.
- `launchctl print-disabled gui/$UID` : format `"label" => disabled`.
  Après `enable`, l'entrée RESTE dans la DB en `"label" => enabled`
  (elle ne disparaît pas) → ne considérer désactivé que `=> disabled`
  (ou `=> true`, format historique).
- Double `bootout` : le second échoue exit **3**, `Boot-out failed: 3:
  No such process` → à tolérer dans delete/disable.
- `kickstart` sur service non chargé : exit 113, `Could not find service`.

## 2026-07-10 — `ScheduleInfo::None` ajouté au modèle

**Décision** : un agent launchd sans `StartCalendarInterval` ni `StartInterval`
(KeepAlive, WatchPaths, MachServices…) porte `ScheduleInfo::None`.
**Raison** : le modèle §3.1 suppose un schedule toujours présent ; c'est faux
pour une grande partie des agents tiers. L'UI affiche « — » et ces agents
restent visibles (et préservés) sans occurrences.

## 2026-07-09 — `Job.next_runs` : `Vec<String>` RFC3339 au lieu de `Vec<DateTime>`

**Décision** : les occurrences transitent par l'IPC en chaînes RFC3339 (offset local),
pas en type date.
**Raison** : specta interdit l'export des types à précision BigInt et l'intégration
chrono ajouterait une dépendance de feature fragile ; le frontend reparse trivialement
du RFC3339 (`new Date(s)`). Le calcul reste en `DateTime<Tz>` côté Rust.

## 2026-07-09 — `StartInterval` : `u32` au lieu de `u64`

**Décision** : `ScheduleInfo::Interval(u32)` / `LaunchdSchedule::Interval(u32)`.
**Raison** : specta refuse u64 (perte de précision JS au-delà de 2^53). u32 couvre
136 ans d'intervalle — largement suffisant. Découvert par le test d'exportabilité
des bindings, qui a rempli exactement son rôle.

## 2026-07-09 — `preview_schedule` renvoie `{ valid, nextRuns }` au lieu de `Vec<String>`

**Décision** : la commande renvoie une structure `SchedulePreview { valid: bool,
next_runs: Vec<String> }`.
**Raison** : pendant la frappe, une expression invalide est un état normal du
formulaire, pas une erreur IPC ; le booléen évite au frontend de deviner si une
liste vide signifie « invalide » ou « non calculable » (@reboot).

## 2026-07-09 — Rust ≥ 1.91 requis (toolchain mise à jour vers 1.97)

**Décision** : le projet requiert une toolchain Rust récente.
**Raison** : specta 2.0.0-rc.25 (imposé par tauri-specta rc.25, seule ligne
compatible Tauri 2) utilise `fmt::from_fn`, stabilisé en Rust 1.91.
