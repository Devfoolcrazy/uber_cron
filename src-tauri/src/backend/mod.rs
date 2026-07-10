//! Trait SchedulerBackend + erreurs typées (§4).

pub mod cron;
pub mod launchd;

use serde::Serialize;
use specta::Type;
use thiserror::Error;

use crate::model::{BackendKind, Diagnostic, Job, JobId, JobSpec, RunResult};

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("job introuvable")]
    NotFound,
    #[error("la crontab a été modifiée par un autre programme depuis le chargement")]
    ConcurrentModification,
    #[error("ligne {line} non analysable")]
    ParseError { line: usize },
    #[error("échec de `{cmd}` : {stderr}")]
    CommandFailed { cmd: String, stderr: String },
    #[error("permission refusée")]
    PermissionDenied,
    #[error("spécification invalide : {0}")]
    InvalidSpec(String),
    #[error("erreur d'E/S : {0}")]
    Io(String),
}

impl From<std::io::Error> for BackendError {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            BackendError::PermissionDenied
        } else {
            BackendError::Io(e.to_string())
        }
    }
}

/// Forme sérialisable exposée à l'IPC (§4) : `code` stable pour la traduction
/// côté frontend (§8.2), `message` = fallback technique.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    pub code: String,
    pub message: String,
    pub detail: Option<String>,
}

impl From<BackendError> for ApiError {
    fn from(e: BackendError) -> Self {
        let (code, detail) = match &e {
            BackendError::NotFound => ("notFound", None),
            BackendError::ConcurrentModification => ("concurrentModification", None),
            BackendError::ParseError { line } => ("parseError", Some(line.to_string())),
            BackendError::CommandFailed { cmd, stderr } => {
                ("commandFailed", Some(format!("{cmd}: {stderr}")))
            }
            BackendError::PermissionDenied => ("permissionDenied", None),
            BackendError::InvalidSpec(d) => ("invalidSpec", Some(d.clone())),
            BackendError::Io(d) => ("io", Some(d.clone())),
        };
        ApiError {
            code: code.to_string(),
            message: e.to_string(),
            detail,
        }
    }
}

pub trait SchedulerBackend {
    fn kind(&self) -> BackendKind;
    fn list(&self) -> Result<Vec<Job>, BackendError>;
    fn get(&self, id: &JobId) -> Result<Job, BackendError>;
    fn create(&self, spec: &JobSpec) -> Result<JobId, BackendError>;
    fn update(&self, id: &JobId, spec: &JobSpec) -> Result<(), BackendError>;
    fn delete(&self, id: &JobId) -> Result<(), BackendError>;
    fn set_enabled(&self, id: &JobId, enabled: bool) -> Result<(), BackendError>;
    fn run_now(&self, id: &JobId) -> Result<RunResult, BackendError>;
    fn diagnostics(&self) -> Vec<Diagnostic>;
}
