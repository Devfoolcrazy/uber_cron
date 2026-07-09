//! Modèle commun (§3). Partagé Rust ↔ TS via tauri-specta.

use serde::{Deserialize, Serialize};
use specta::Type;

/// Identifiant de job. launchd : le Label. cron : "{hash_snapshot}:{index}:{hash_ligne}"
/// (opaque pour le frontend ; invalidé par toute mutation, §3.2).
pub type JobId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum BackendKind {
    Cron,
    Launchd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    /// launchd : process en cours d'exécution.
    Running,
    /// launchd : chargé dans le domaine gui, pas en cours d'exécution.
    Loaded,
    /// launchd : non chargé.
    NotLoaded,
    /// cron : pas de notion d'état runtime.
    Static,
    /// launchd : `launchctl print` inexploitable (parsing défensif, §6.2).
    Unknown,
}

/// Une entrée de StartCalendarInterval. Champs absents = "toutes les valeurs".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEntry {
    pub minute: Option<u8>,
    pub hour: Option<u8>,
    pub day: Option<u8>,
    /// 0-7, 0 et 7 = dimanche (normalisé à 0 au parsing, §11).
    pub weekday: Option<u8>,
    pub month: Option<u8>,
}

/// Représentation structurée du schedule — le frontend en dérive la phrase humaine (§3.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum ScheduleInfo {
    /// Expression 5 champs ou @-raccourci (@daily, @reboot...).
    CronExpr(String),
    CalendarIntervals(Vec<CalendarEntry>),
    /// StartInterval en secondes.
    Interval(u64),
}

/// Modèle commun en lecture (§3.1).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: JobId,
    pub backend: BackendKind,
    pub label: String,
    pub command: String,
    pub schedule: ScheduleInfo,
    pub schedule_raw: String,
    pub enabled: bool,
    /// 5 prochaines occurrences, RFC3339 avec offset local. Vide si non calculable.
    pub next_runs: Vec<String>,
    pub status: JobStatus,
    /// launchd : extrait de `launchctl print` (défensif). cron : None.
    pub last_exit_code: Option<i32>,
    /// launchd : label préfixé com.ubercron. — cron : toujours true (§3.1).
    pub managed: bool,
}

/// Spécification d'édition, par backend — PAS de formulaire unifié (§3.3).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum JobSpec {
    Cron(CronJobSpec),
    Launchd(LaunchdJobSpec),
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CronJobSpec {
    /// Expression 5 champs (l'éditeur ne crée pas de @-raccourcis, §5.1).
    pub schedule: String,
    pub command: String,
    /// Écrit en commentaire `# name: ...` sur la ligne précédente (§5.1).
    pub name: Option<String>,
}

/// Mode de commande launchd (§6.3) : wrapper shell par défaut, ou argv explicite.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum LaunchdCommand {
    ShellWrapper(String),
    Argv(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum LaunchdSchedule {
    CalendarIntervals(Vec<CalendarEntry>),
    Interval(u64),
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LaunchdJobSpec {
    pub label: String,
    pub command: LaunchdCommand,
    pub schedule: LaunchdSchedule,
    pub run_at_load: bool,
    pub stdout_path: Option<String>,
    pub stderr_path: Option<String>,
}

/// Résultat d'un "Exécuter maintenant" (§4).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum RunResult {
    /// cron : exécution directe terminée.
    Completed {
        exit_code: i32,
        stdout_tail: String,
        stderr_tail: String,
    },
    /// launchd : kickstart asynchrone — consulter statut/logs.
    Started,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    Ok,
    Warning,
    Error,
}

/// Un check de diagnostic (§8). `code` est stable : le frontend le traduit (§8.2) ;
/// `detail` porte du contexte brut non traduisible (chemin, stderr...).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub detail: Option<String>,
}
