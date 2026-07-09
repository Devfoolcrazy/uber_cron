//! Commands Tauri : couche fine, zéro logique métier (§7).

use std::sync::Arc;

use serde::Serialize;
use specta::Type;
use tauri::State;

use crate::backend::cron::backend::CronBackend;
use crate::backend::cron::occurrence;
use crate::backend::{ApiError, BackendError, SchedulerBackend};
use crate::model::{BackendKind, Diagnostic, Job, JobId, JobSpec, RunResult, ScheduleInfo};

pub struct AppState {
    pub cron: Arc<CronBackend>,
}

impl AppState {
    /// Résout le backend demandé. launchd arrive aux étapes 7-10 (§10).
    fn backend(&self, kind: BackendKind) -> Result<Arc<dyn SchedulerBackend + Send + Sync>, ApiError> {
        match kind {
            BackendKind::Cron => Ok(self.cron.clone()),
            BackendKind::Launchd => Err(ApiError {
                code: "notImplemented".to_string(),
                message: "backend launchd non disponible dans cette version".to_string(),
                detail: None,
            }),
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
    let b = state.backend(backend)?;
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
    let b = state.backend(backend)?;
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
    let b = state.backend(backend)?;
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
    let b = state.backend(backend)?;
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
    let b = state.backend(backend)?;
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
    let b = state.backend(backend)?;
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
    let b = state.backend(backend)?;
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
    let b = state.backend(backend)?;
    tauri::async_runtime::spawn_blocking(move || b.diagnostics())
        .await
        .map_err(|e| ApiError::from(BackendError::Io(e.to_string())))
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
        // launchd : étapes 7-10.
        ScheduleInfo::CalendarIntervals(_) | ScheduleInfo::Interval(_) => SchedulePreview {
            valid: false,
            next_runs: Vec::new(),
        },
    }
}
