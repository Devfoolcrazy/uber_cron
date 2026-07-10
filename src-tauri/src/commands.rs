//! Commands Tauri : couche fine, zéro logique métier (§7).

use std::sync::Arc;

use serde::Serialize;
use specta::Type;
use tauri::State;

use crate::backend::cron::backend::CronBackend;
use crate::backend::cron::occurrence;
use crate::backend::launchd::backend::LaunchdBackend;
use crate::backend::{ApiError, BackendError, SchedulerBackend};
use crate::model::{BackendKind, Diagnostic, Job, JobId, JobSpec, RunResult, ScheduleInfo};

pub struct AppState {
    pub cron: Arc<CronBackend>,
    pub launchd: Arc<LaunchdBackend>,
}

impl AppState {
    fn backend(&self, kind: BackendKind) -> Arc<dyn SchedulerBackend + Send + Sync> {
        match kind {
            BackendKind::Cron => self.cron.clone(),
            BackendKind::Launchd => self.launchd.clone(),
        }
    }
}

fn api<T>(r: Result<T, BackendError>) -> Result<T, ApiError> {
    r.map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
#[specta::specta]
pub async fn list_jobs(
    state: State<'_, AppState>,
    backend: BackendKind,
) -> Result<Vec<Job>, ApiError> {
    let b = state.backend(backend);
    api(tauri::async_runtime::spawn_blocking(move || b.list())
        .await
        .map_err(|e| BackendError::Io(e.to_string()))?)
}

#[tauri::command]
#[specta::specta]
pub async fn get_job(
    state: State<'_, AppState>,
    backend: BackendKind,
    id: JobId,
) -> Result<Job, ApiError> {
    let b = state.backend(backend);
    api(tauri::async_runtime::spawn_blocking(move || b.get(&id))
        .await
        .map_err(|e| BackendError::Io(e.to_string()))?)
}

#[tauri::command]
#[specta::specta]
pub async fn create_job(
    state: State<'_, AppState>,
    backend: BackendKind,
    spec: JobSpec,
) -> Result<JobId, ApiError> {
    let b = state.backend(backend);
    api(
        tauri::async_runtime::spawn_blocking(move || b.create(&spec))
            .await
            .map_err(|e| BackendError::Io(e.to_string()))?,
    )
}

#[tauri::command]
#[specta::specta]
pub async fn update_job(
    state: State<'_, AppState>,
    backend: BackendKind,
    id: JobId,
    spec: JobSpec,
) -> Result<(), ApiError> {
    let b = state.backend(backend);
    api(
        tauri::async_runtime::spawn_blocking(move || b.update(&id, &spec))
            .await
            .map_err(|e| BackendError::Io(e.to_string()))?,
    )
}

#[tauri::command]
#[specta::specta]
pub async fn delete_job(
    state: State<'_, AppState>,
    backend: BackendKind,
    id: JobId,
) -> Result<(), ApiError> {
    let b = state.backend(backend);
    api(tauri::async_runtime::spawn_blocking(move || b.delete(&id))
        .await
        .map_err(|e| BackendError::Io(e.to_string()))?)
}

#[tauri::command]
#[specta::specta]
pub async fn set_job_enabled(
    state: State<'_, AppState>,
    backend: BackendKind,
    id: JobId,
    enabled: bool,
) -> Result<(), ApiError> {
    let b = state.backend(backend);
    api(
        tauri::async_runtime::spawn_blocking(move || b.set_enabled(&id, enabled))
            .await
            .map_err(|e| BackendError::Io(e.to_string()))?,
    )
}

/// Exécution à la demande (§5.5 / §6.6) — potentiellement longue, toujours hors
/// du thread principal.
#[tauri::command]
#[specta::specta]
pub async fn run_job(
    state: State<'_, AppState>,
    backend: BackendKind,
    id: JobId,
) -> Result<RunResult, ApiError> {
    let b = state.backend(backend);
    api(tauri::async_runtime::spawn_blocking(move || b.run_now(&id))
        .await
        .map_err(|e| BackendError::Io(e.to_string()))?)
}

#[tauri::command]
#[specta::specta]
pub async fn run_diagnostics(
    state: State<'_, AppState>,
    backend: BackendKind,
) -> Result<Vec<Diagnostic>, ApiError> {
    let b = state.backend(backend);
    tauri::async_runtime::spawn_blocking(move || b.diagnostics())
        .await
        .map_err(|e| ApiError::from(BackendError::Io(e.to_string())))
}

/// Visionneuse de logs (§8.1) : dernières lignes d'un fichier de log
/// launchd (tail simple, pas de suivi live au MVP). `$HOME` accepté en tête
/// de chemin (convention des suggestions de journal).
#[tauri::command]
#[specta::specta]
pub fn read_log_tail(path: String) -> Result<String, ApiError> {
    const TAIL_BYTES: u64 = 16 * 1024;

    let expanded = if let Some(rest) = path.strip_prefix("$HOME/") {
        dirs::home_dir()
            .unwrap_or_default()
            .join(rest)
            .to_string_lossy()
            .into_owned()
    } else if let Some(rest) = path.strip_prefix("~/") {
        dirs::home_dir()
            .unwrap_or_default()
            .join(rest)
            .to_string_lossy()
            .into_owned()
    } else {
        path
    };

    let content = (|| -> std::io::Result<String> {
        use std::io::{Read, Seek, SeekFrom};
        let mut file = std::fs::File::open(&expanded)?;
        let len = file.metadata()?.len();
        if len > TAIL_BYTES {
            file.seek(SeekFrom::End(-(TAIL_BYTES as i64)))?;
        }
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        Ok(String::from_utf8_lossy(&buf).into_owned())
    })()
    .map_err(|e| ApiError::from(BackendError::from(e)))?;
    Ok(content)
}

/// Aperçu live du formulaire (§7). La phrase humaine est calculée côté frontend ;
/// ici uniquement validité + prochaines occurrences.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SchedulePreview {
    pub valid: bool,
    pub next_runs: Vec<String>,
}

#[tauri::command]
#[specta::specta]
pub fn preview_schedule(schedule: ScheduleInfo) -> SchedulePreview {
    match schedule {
        ScheduleInfo::CronExpr(expr) => SchedulePreview {
            valid: occurrence::is_valid(&expr),
            next_runs: occurrence::next_runs_rfc3339(&expr, 5),
        },
        ScheduleInfo::CalendarIntervals(entries) => SchedulePreview {
            valid: !entries.is_empty(),
            next_runs: crate::backend::launchd::calendar::next_runs_rfc3339(&entries, 5),
        },
        // Interval : pas de dates absolues (§6.7) ; None : pas de schedule.
        ScheduleInfo::Interval(secs) => SchedulePreview {
            valid: secs > 0,
            next_runs: Vec::new(),
        },
        ScheduleInfo::None => SchedulePreview {
            valid: false,
            next_runs: Vec::new(),
        },
    }
}
