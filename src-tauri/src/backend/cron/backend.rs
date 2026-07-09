//! CronBackend (§5) : CRUD sur la crontab utilisateur via `crontab -l` / `crontab -`.
//!
//! Identité (§3.2) : JobId = "{hash_snapshot}:{index}:{hash_ligne}". Toute écriture
//! relit la crontab et vérifie le hash du snapshot — sinon ConcurrentModification.
//! Contrat : toute mutation réussie invalide TOUS les JobId cron du frontend.

use std::path::PathBuf;
use std::sync::Arc;

use sha2::{Digest, Sha256};

use crate::backend::cron::occurrence;
use crate::backend::cron::parser::{canonicalize_job_parts, Crontab, CrontabLine};
use crate::backend::{BackendError, SchedulerBackend};
use crate::model::{
    BackendKind, CronJobSpec, Diagnostic, DiagnosticSeverity, Job, JobId, JobSpec, JobStatus,
    RunResult, ScheduleInfo,
};
use crate::system::SystemCommands;

const CRONTAB: &str = "/usr/bin/crontab";
const SHELL: &str = "/bin/sh";
const MAX_BACKUPS: usize = 20;
/// Queue de sortie conservée par run_now (dernier Ko utile pour le diagnostic).
const TAIL_BYTES: usize = 8 * 1024;

pub struct CronBackend {
    system: Arc<dyn SystemCommands>,
    backups_dir: PathBuf,
}

impl CronBackend {
    pub fn new(system: Arc<dyn SystemCommands>, backups_dir: PathBuf) -> Self {
        Self {
            system,
            backups_dir,
        }
    }

    /// Lit la crontab. Exit != 0 + "no crontab for" ⇒ crontab vide, pas une erreur (§5.1).
    fn read_crontab(&self) -> Result<String, BackendError> {
        let out = self.system.run(CRONTAB, &["-l"], None)?;
        if out.status == 0 {
            Ok(out.stdout)
        } else if out.stderr.contains("no crontab for") {
            Ok(String::new())
        } else {
            Err(BackendError::CommandFailed {
                cmd: format!("{CRONTAB} -l"),
                stderr: out.stderr,
            })
        }
    }

    fn write_crontab(&self, current: &str, new_text: &str) -> Result<(), BackendError> {
        self.backup(current)?;
        let out = self.system.run(CRONTAB, &["-"], Some(new_text))?;
        if out.status != 0 {
            return Err(BackendError::CommandFailed {
                cmd: format!("{CRONTAB} -"),
                stderr: out.stderr,
            });
        }
        Ok(())
    }

    /// Backup horodaté du contenu courant, avant toute écriture (§5.2). Garde les
    /// MAX_BACKUPS plus récents. Un échec de backup bloque l'écriture (sécurité d'abord).
    fn backup(&self, current: &str) -> Result<(), BackendError> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);

        std::fs::create_dir_all(&self.backups_dir)?;
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        std::fs::write(
            self.backups_dir
                .join(format!("crontab-{stamp}-{seq:06}.txt")),
            current,
        )?;

        let mut backups: Vec<PathBuf> = std::fs::read_dir(&self.backups_dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("crontab-") && n.ends_with(".txt"))
            })
            .collect();
        backups.sort();
        while backups.len() > MAX_BACKUPS {
            let oldest = backups.remove(0);
            let _ = std::fs::remove_file(oldest);
        }
        Ok(())
    }

    fn snapshot_hash(text: &str) -> String {
        hex_prefix(&Sha256::digest(text.as_bytes()))
    }

    fn line_hash(line: &CrontabLine) -> String {
        let mut hasher = Sha256::new();
        for raw in line.raw_lines() {
            hasher.update(raw.as_bytes());
            hasher.update(b"\n");
        }
        hex_prefix(&hasher.finalize())
    }

    fn make_id(snapshot: &str, index: usize, line: &CrontabLine) -> JobId {
        format!("{snapshot}:{index}:{}", Self::line_hash(line))
    }

    /// Relit la crontab et résout un JobId : hash de snapshot ET hash de ligne doivent
    /// correspondre, sinon ConcurrentModification (§3.2).
    fn resolve(&self, id: &JobId) -> Result<(String, Crontab, usize), BackendError> {
        let (snapshot, index, line_hash) = parse_id(id)?;
        let text = self.read_crontab()?;
        if Self::snapshot_hash(&text) != snapshot {
            return Err(BackendError::ConcurrentModification);
        }
        let ct = Crontab::parse(&text);
        let line = ct.lines.get(index).ok_or(BackendError::NotFound)?;
        if !matches!(
            line,
            CrontabLine::Job { .. } | CrontabLine::DisabledJob { .. }
        ) {
            return Err(BackendError::NotFound);
        }
        if Self::line_hash(line) != line_hash {
            return Err(BackendError::ConcurrentModification);
        }
        Ok((text, ct, index))
    }

    fn job_from_line(&self, snapshot: &str, index: usize, line: &CrontabLine) -> Option<Job> {
        let (schedule, command, name, enabled) = match line {
            CrontabLine::Job {
                schedule,
                command,
                name,
                ..
            } => (schedule, command, name, true),
            CrontabLine::DisabledJob {
                schedule,
                command,
                name,
                ..
            } => (schedule, command, name, false),
            _ => return None,
        };
        let label = name.clone().unwrap_or_else(|| truncate(command, 48));
        Some(Job {
            id: Self::make_id(snapshot, index, line),
            backend: BackendKind::Cron,
            label,
            name: name.clone(),
            command: command.clone(),
            schedule: ScheduleInfo::CronExpr(schedule.clone()),
            schedule_raw: schedule.clone(),
            enabled,
            next_runs: occurrence::next_runs_rfc3339(schedule, 5),
            status: JobStatus::Static,
            last_exit_code: None,
            managed: true,
        })
    }

    fn cron_spec(spec: &JobSpec) -> Result<&CronJobSpec, BackendError> {
        match spec {
            JobSpec::Cron(s) => Ok(s),
            JobSpec::Launchd(_) => Err(BackendError::InvalidSpec(
                "spec launchd envoyée au backend cron".to_string(),
            )),
        }
    }

    fn canonicalize(spec: &CronJobSpec) -> Result<(String, String, Option<&str>), BackendError> {
        let name = spec
            .name
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty());
        let (schedule, command) = canonicalize_job_parts(&spec.schedule, &spec.command, name)
            .ok_or_else(|| {
                BackendError::InvalidSpec("expression ou commande invalide".to_string())
            })?;
        // Validation sémantique (plages de valeurs) par croner, en plus de la forme.
        if !occurrence::is_valid(&schedule) {
            return Err(BackendError::InvalidSpec(format!(
                "expression cron invalide : {schedule}"
            )));
        }
        Ok((schedule, command, name))
    }
}

impl SchedulerBackend for CronBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::Cron
    }

    fn list(&self) -> Result<Vec<Job>, BackendError> {
        let text = self.read_crontab()?;
        let snapshot = Self::snapshot_hash(&text);
        let ct = Crontab::parse(&text);
        Ok(ct
            .lines
            .iter()
            .enumerate()
            .filter_map(|(i, line)| self.job_from_line(&snapshot, i, line))
            .collect())
    }

    fn get(&self, id: &JobId) -> Result<Job, BackendError> {
        let (text, ct, index) = self.resolve(id)?;
        let snapshot = Self::snapshot_hash(&text);
        self.job_from_line(&snapshot, index, &ct.lines[index])
            .ok_or(BackendError::NotFound)
    }

    fn create(&self, spec: &JobSpec) -> Result<JobId, BackendError> {
        let spec = Self::cron_spec(spec)?;
        let (schedule, command, name) = Self::canonicalize(spec)?;
        let text = self.read_crontab()?;
        let mut ct = Crontab::parse(&text);
        ct.push_job(&schedule, &command, name);
        let new_text = ct.serialize();
        self.write_crontab(&text, &new_text)?;
        let index = ct.lines.len() - 1;
        Ok(Self::make_id(
            &Self::snapshot_hash(&new_text),
            index,
            &ct.lines[index],
        ))
    }

    fn update(&self, id: &JobId, spec: &JobSpec) -> Result<(), BackendError> {
        let spec = Self::cron_spec(spec)?;
        let (schedule, command, name) = Self::canonicalize(spec)?;
        let (text, mut ct, index) = self.resolve(id)?;
        ct.replace_job(index, &schedule, &command, name)
            .ok_or(BackendError::NotFound)?;
        self.write_crontab(&text, &ct.serialize())
    }

    fn delete(&self, id: &JobId) -> Result<(), BackendError> {
        let (text, mut ct, index) = self.resolve(id)?;
        ct.remove_job(index).ok_or(BackendError::NotFound)?;
        self.write_crontab(&text, &ct.serialize())
    }

    fn set_enabled(&self, id: &JobId, enabled: bool) -> Result<(), BackendError> {
        let (text, mut ct, index) = self.resolve(id)?;
        ct.set_job_disabled(index, !enabled)
            .ok_or(BackendError::NotFound)?;
        self.write_crontab(&text, &ct.serialize())
    }

    /// Exécution directe via `/bin/sh -c` (§5.5). Ne modifie jamais la crontab.
    /// L'environnement est celui de l'app, pas celui de cron — l'UI l'indique.
    fn run_now(&self, id: &JobId) -> Result<RunResult, BackendError> {
        let (_, ct, index) = self.resolve(id)?;
        let command = match &ct.lines[index] {
            CrontabLine::Job { command, .. } | CrontabLine::DisabledJob { command, .. } => {
                command.clone()
            }
            _ => return Err(BackendError::NotFound),
        };
        let out = self.system.run(SHELL, &["-c", &command], None)?;
        Ok(RunResult::Completed {
            exit_code: out.status,
            stdout_tail: tail(&out.stdout),
            stderr_tail: tail(&out.stderr),
        })
    }

    fn diagnostics(&self) -> Vec<Diagnostic> {
        let crontab_check = match self.read_crontab() {
            Ok(_) => Diagnostic {
                severity: DiagnosticSeverity::Ok,
                code: "cron.crontabAccessible".to_string(),
                detail: None,
            },
            Err(e) => Diagnostic {
                severity: DiagnosticSeverity::Error,
                code: "cron.crontabInaccessible".to_string(),
                detail: Some(e.to_string()),
            },
        };
        vec![
            crontab_check,
            // Avertissements informatifs permanents (§8.1) — le frontend les traduit.
            Diagnostic {
                severity: DiagnosticSeverity::Warning,
                code: "cron.fullDiskAccess".to_string(),
                detail: Some("/usr/sbin/cron".to_string()),
            },
            Diagnostic {
                severity: DiagnosticSeverity::Warning,
                code: "cron.minimalPath".to_string(),
                detail: None,
            },
        ]
    }
}

fn parse_id(id: &JobId) -> Result<(String, usize, String), BackendError> {
    let mut parts = id.split(':');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(snap), Some(idx), Some(line), None) => {
            let index = idx.parse().map_err(|_| BackendError::NotFound)?;
            Ok((snap.to_string(), index, line.to_string()))
        }
        _ => Err(BackendError::NotFound),
    }
}

fn hex_prefix(digest: &[u8]) -> String {
    digest[..8].iter().map(|b| format!("{b:02x}")).collect()
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{cut}…")
    }
}

/// Conserve la fin de la sortie (la plus utile au diagnostic), bornée en octets.
fn tail(s: &str) -> String {
    if s.len() <= TAIL_BYTES {
        return s.to_string();
    }
    let start = s.len() - TAIL_BYTES;
    let boundary = (start..s.len())
        .find(|&i| s.is_char_boundary(i))
        .unwrap_or(start);
    format!("[…]\n{}", &s[boundary..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::mock::MockSystem;

    const NO_CRONTAB: (i32, &str, &str) = (1, "", "crontab: no crontab for moi");

    fn backend() -> (Arc<MockSystem>, CronBackend, tempfile::TempDir) {
        let system = Arc::new(MockSystem::new());
        let dir = tempfile::tempdir().unwrap();
        let backend = CronBackend::new(system.clone(), dir.path().join("backups"));
        (system, backend, dir)
    }

    fn spec(schedule: &str, command: &str, name: Option<&str>) -> JobSpec {
        JobSpec::Cron(CronJobSpec {
            schedule: schedule.to_string(),
            command: command.to_string(),
            name: name.map(str::to_string),
        })
    }

    /// stdin du dernier appel `crontab -` enregistré par le mock.
    fn last_written(system: &MockSystem) -> String {
        system
            .calls()
            .iter()
            .rev()
            .find(|c| c.program == CRONTAB && c.args == ["-"])
            .and_then(|c| c.stdin.clone())
            .expect("aucune écriture crontab enregistrée")
    }

    #[test]
    fn crontab_absente_donne_liste_vide() {
        let (system, backend, _dir) = backend();
        system.push_response(NO_CRONTAB.0, NO_CRONTAB.1, NO_CRONTAB.2);
        assert!(backend.list().unwrap().is_empty());
    }

    #[test]
    fn autre_erreur_crontab_est_remontee() {
        let (system, backend, _dir) = backend();
        system.push_response(1, "", "crontab: fatal: quelque chose");
        assert!(matches!(
            backend.list(),
            Err(BackendError::CommandFailed { .. })
        ));
    }

    #[test]
    fn list_expose_noms_etats_et_ignore_le_reste() {
        let (system, backend, _dir) = backend();
        system.push_response(
            0,
            "SHELL=/bin/sh\n# name: Sauvegarde\n0 3 * * * /usr/bin/backup\n# UBERCRON-DISABLED: @daily /usr/bin/cleanup\nligne invalide\n",
            "",
        );
        let jobs = backend.list().unwrap();
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].label, "Sauvegarde");
        assert!(jobs[0].enabled);
        assert_eq!(jobs[1].schedule_raw, "@daily");
        assert!(!jobs[1].enabled);
        assert!(jobs[1].managed);
    }

    #[test]
    fn create_ajoute_en_fin_et_ecrit_via_stdin() {
        let (system, backend, dir) = backend();
        system.push_response(0, "0 1 * * * /usr/bin/existant\n", "");
        system.push_response(0, "", ""); // crontab -
        backend
            .create(&spec("30 8 * * 1-5", "/usr/bin/cafe", Some("Café")))
            .unwrap();
        assert_eq!(
            last_written(&system),
            "0 1 * * * /usr/bin/existant\n# name: Café\n30 8 * * 1-5 /usr/bin/cafe\n"
        );
        // Backup créé avant l'écriture (§5.2).
        let backups: Vec<_> = std::fs::read_dir(dir.path().join("backups"))
            .unwrap()
            .collect();
        assert_eq!(backups.len(), 1);
    }

    #[test]
    fn create_rejette_l_injection_de_newline() {
        let (_, backend, _dir) = backend();
        let err = backend
            .create(&spec("0 1 * * *", "echo pwned\n* * * * * mal", None))
            .unwrap_err();
        assert!(matches!(err, BackendError::InvalidSpec(_)));
    }

    #[test]
    fn create_rejette_le_schedule_excedentaire() {
        let (_, backend, _dir) = backend();
        // 6 champs : le 6e serait avalé par la commande — refusé.
        let err = backend
            .create(&spec("0 1 * * * 1", "/usr/bin/cmd", None))
            .unwrap_err();
        assert!(matches!(err, BackendError::InvalidSpec(_)));
    }

    #[test]
    fn update_ne_reformate_que_la_ligne_editee() {
        let (system, backend, _dir) = backend();
        let text = "# commentaire   conservé\n0 1 * * *    /usr/bin/espaces   bizarres\n@daily /usr/bin/cible\n";
        system.push_response(0, text, "");
        let id = backend.list().unwrap()[1].id.clone();
        system.push_response(0, text, ""); // relecture par resolve()
        system.push_response(0, "", ""); // crontab -
        backend
            .update(&id, &spec("15 4 * * *", "/usr/bin/cible --v2", None))
            .unwrap();
        assert_eq!(
            last_written(&system),
            "# commentaire   conservé\n0 1 * * *    /usr/bin/espaces   bizarres\n15 4 * * * /usr/bin/cible --v2\n"
        );
    }

    #[test]
    fn delete_supprime_le_job_et_son_nom() {
        let (system, backend, _dir) = backend();
        let text = "# name: À supprimer\n0 3 * * * /usr/bin/bye\n0 4 * * * /usr/bin/reste\n";
        system.push_response(0, text, "");
        let id = backend.list().unwrap()[0].id.clone();
        system.push_response(0, text, "");
        system.push_response(0, "", "");
        backend.delete(&id).unwrap();
        assert_eq!(last_written(&system), "0 4 * * * /usr/bin/reste\n");
    }

    #[test]
    fn disable_puis_enable_restitue_le_texte_original() {
        let (system, backend, _dir) = backend();
        let original = "0 3 * * *   /usr/bin/backup   --espaces\n";
        system.push_response(0, original, "");
        let id = backend.list().unwrap()[0].id.clone();
        system.push_response(0, original, "");
        system.push_response(0, "", "");
        backend.set_enabled(&id, false).unwrap();
        let disabled_text = last_written(&system);
        assert_eq!(
            disabled_text,
            "# UBERCRON-DISABLED: 0 3 * * *   /usr/bin/backup   --espaces\n"
        );

        // Nouveau snapshot : nouvel id (contrat d'invalidation §3.2).
        system.push_response(0, &disabled_text, "");
        let id = backend.list().unwrap()[0].id.clone();
        system.push_response(0, &disabled_text, "");
        system.push_response(0, "", "");
        backend.set_enabled(&id, true).unwrap();
        assert_eq!(last_written(&system), original);
    }

    #[test]
    fn edition_concurrente_detectee() {
        let (system, backend, _dir) = backend();
        system.push_response(0, "0 3 * * * /usr/bin/backup\n", "");
        let id = backend.list().unwrap()[0].id.clone();
        // Entre-temps, quelqu'un a modifié la crontab.
        system.push_response(0, "0 3 * * * /usr/bin/backup\n# autre chose\n", "");
        assert!(matches!(
            backend.delete(&id),
            Err(BackendError::ConcurrentModification)
        ));
        // Et aucune écriture n'a eu lieu.
        assert!(!system
            .calls()
            .iter()
            .any(|c| c.program == CRONTAB && c.args == ["-"]));
    }

    #[test]
    fn run_now_execute_via_sh_sans_toucher_la_crontab() {
        let (system, backend, _dir) = backend();
        let text = "0 3 * * * /usr/bin/backup --full\n";
        system.push_response(0, text, "");
        let id = backend.list().unwrap()[0].id.clone();
        system.push_response(0, text, "");
        system.push_response(2, "sortie", "erreur");
        let result = backend.run_now(&id).unwrap();
        match result {
            RunResult::Completed {
                exit_code,
                stdout_tail,
                stderr_tail,
            } => {
                assert_eq!(exit_code, 2);
                assert_eq!(stdout_tail, "sortie");
                assert_eq!(stderr_tail, "erreur");
            }
            RunResult::Started => panic!("attendu Completed"),
        }
        let calls = system.calls();
        let sh = calls.last().unwrap();
        assert_eq!(sh.program, SHELL);
        assert_eq!(sh.args, ["-c", "/usr/bin/backup --full"]);
        assert!(!calls.iter().any(|c| c.program == CRONTAB && c.args == ["-"]));
    }

    #[test]
    fn backups_limites_a_20() {
        let (_, backend, dir) = backend();
        for i in 0..25 {
            backend.backup(&format!("contenu {i}\n")).unwrap();
        }
        let mut names: Vec<String> = std::fs::read_dir(dir.path().join("backups"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names.len(), MAX_BACKUPS);
        // Les plus anciens ont été élagués : le plus vieux restant contient "contenu 5".
        let oldest = std::fs::read_to_string(dir.path().join("backups").join(&names[0])).unwrap();
        assert_eq!(oldest, "contenu 5\n");
    }

    #[test]
    fn diagnostics_couvre_acces_fda_et_path() {
        let (system, backend, _dir) = backend();
        system.push_response(NO_CRONTAB.0, NO_CRONTAB.1, NO_CRONTAB.2);
        let diags = backend.diagnostics();
        assert_eq!(diags.len(), 3);
        assert_eq!(diags[0].code, "cron.crontabAccessible");
        assert_eq!(diags[0].severity, DiagnosticSeverity::Ok);
        assert_eq!(diags[1].code, "cron.fullDiskAccess");
        assert_eq!(diags[2].code, "cron.minimalPath");
    }
}
