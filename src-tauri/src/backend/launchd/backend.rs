//! LaunchdBackend (§6) : source de vérité = les fichiers plist ; launchctl ne
//! sert qu'à charger/décharger, activer/désactiver (override DB, §6.5) et
//! lire le statut runtime (parsing défensif, §6.2).
//!
//! Séquences validées par le spike du 2026-07-10 (DECISIONS.md) :
//! - enable AVANT bootstrap (sinon « Bootstrap failed: 5: Input/output error ») ;
//! - bootout toléré quand le service n'est pas chargé (exit 3) ;
//! - print exit 0 = chargé, 113 = non chargé.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::backend::launchd::calendar;
use crate::backend::launchd::launchctl::{parse_print, parse_print_disabled, PrintStatus};
use crate::backend::{BackendError, SchedulerBackend};
use crate::model::{
    BackendKind, Diagnostic, DiagnosticSeverity, Job, JobId, JobSpec, JobStatus, LaunchdCommand,
    LaunchdJobSpec, LaunchdSchedule, RunResult, ScheduleInfo,
};
use crate::system::SystemCommands;

const LAUNCHCTL: &str = "/bin/launchctl";
pub const MANAGED_PREFIX: &str = "com.ubercron.";
const MAX_BACKUPS_PER_LABEL: usize = 20;

pub struct LaunchdBackend {
    system: Arc<dyn SystemCommands>,
    agents_dir: PathBuf,
    backups_dir: PathBuf,
    trash_dir: PathBuf,
    uid: u32,
}

impl LaunchdBackend {
    pub fn new(
        system: Arc<dyn SystemCommands>,
        agents_dir: PathBuf,
        backups_dir: PathBuf,
        trash_dir: PathBuf,
        uid: u32,
    ) -> Self {
        Self {
            system,
            agents_dir,
            backups_dir,
            trash_dir,
            uid,
        }
    }

    fn target(&self, label: &str) -> String {
        format!("gui/{}/{label}", self.uid)
    }

    fn domain(&self) -> String {
        format!("gui/{}", self.uid)
    }

    fn launchctl(&self, args: &[&str]) -> Result<(i32, String, String), BackendError> {
        let out = self.system.run(LAUNCHCTL, args, None)?;
        Ok((out.status, out.stdout, out.stderr))
    }

    /// Échec → BackendError::CommandFailed avec la sous-commande en clair.
    fn launchctl_ok(&self, args: &[&str]) -> Result<(), BackendError> {
        let (status, _, stderr) = self.launchctl(args)?;
        if status != 0 {
            return Err(BackendError::CommandFailed {
                cmd: format!("launchctl {}", args.join(" ")),
                stderr,
            });
        }
        Ok(())
    }

    /// bootout toléré : exit 3 « No such process » quand non chargé (spike).
    fn bootout_tolerant(&self, label: &str) -> Result<(), BackendError> {
        let (_, _, _) = self.launchctl(&["bootout", &self.target(label)])?;
        Ok(())
    }

    fn print_status(&self, label: &str) -> PrintStatus {
        match self.launchctl(&["print", &self.target(label)]) {
            Ok((status, stdout, _)) => parse_print(status, &stdout),
            Err(_) => PrintStatus {
                loaded: false,
                running: None,
                last_exit_code: None,
            },
        }
    }

    /// Labels désactivés dans l'override DB. Échec → ensemble vide (défensif).
    fn disabled_labels(&self) -> HashSet<String> {
        match self.launchctl(&["print-disabled", &self.domain()]) {
            Ok((0, stdout, _)) => parse_print_disabled(&stdout),
            _ => HashSet::new(),
        }
    }

    /// Scan trié de *.plist ; (chemin, valeur) analysables + noms illisibles.
    #[allow(clippy::type_complexity)]
    fn scan(&self) -> Result<(Vec<(PathBuf, plist::Value)>, Vec<String>), BackendError> {
        let mut paths: Vec<PathBuf> = std::fs::read_dir(&self.agents_dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("plist"))
            .collect();
        paths.sort();
        let mut parsed = Vec::new();
        let mut unreadable = Vec::new();
        for path in paths {
            match plist::Value::from_file(&path) {
                Ok(value) => parsed.push((path, value)),
                Err(_) => unreadable.push(
                    path.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                ),
            }
        }
        Ok((parsed, unreadable))
    }

    fn find_by_label(&self, label: &str) -> Result<(PathBuf, plist::Value), BackendError> {
        let (parsed, _) = self.scan()?;
        parsed
            .into_iter()
            .find(|(_, v)| label_of(v).as_deref() == Some(label))
            .ok_or(BackendError::NotFound)
    }

    fn job_from_plist(
        &self,
        value: &plist::Value,
        disabled: &HashSet<String>,
    ) -> Option<Job> {
        let dict = value.as_dictionary()?;
        let label = label_of(value)?;

        let command = command_display(dict);
        let (schedule, schedule_raw) = schedule_of(dict);
        let launchd_spec = spec_of(dict, &label, &schedule);

        let next_runs = match &schedule {
            ScheduleInfo::CalendarIntervals(entries) => calendar::next_runs_rfc3339(entries, 5),
            _ => Vec::new(),
        };

        let plist_disabled = dict
            .get("Disabled")
            .and_then(|v| v.as_boolean())
            .unwrap_or(false);
        let enabled = !disabled.contains(&label) && !plist_disabled;

        let status = self.print_status(&label);
        let job_status = if !status.loaded {
            JobStatus::NotLoaded
        } else {
            match status.running {
                Some(true) => JobStatus::Running,
                Some(false) => JobStatus::Loaded,
                None => JobStatus::Unknown,
            }
        };

        Some(Job {
            id: label.clone(),
            backend: BackendKind::Launchd,
            managed: label.starts_with(MANAGED_PREFIX),
            label,
            name: None,
            command,
            schedule,
            schedule_raw,
            enabled,
            next_runs,
            status: job_status,
            last_exit_code: status.last_exit_code,
            launchd_spec,
        })
    }

    fn launchd_spec(spec: &JobSpec) -> Result<&LaunchdJobSpec, BackendError> {
        match spec {
            JobSpec::Launchd(s) => Ok(s),
            JobSpec::Cron(_) => Err(BackendError::InvalidSpec(
                "spec cron envoyée au backend launchd".to_string(),
            )),
        }
    }

    fn plist_path(&self, label: &str) -> PathBuf {
        self.agents_dir.join(format!("{label}.plist"))
    }

    /// Copie horodatée avant update (§6.4), élaguée par label.
    fn backup_plist(&self, label: &str, path: &Path) -> Result<(), BackendError> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);

        std::fs::create_dir_all(&self.backups_dir)?;
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        std::fs::copy(
            path,
            self.backups_dir
                .join(format!("{label}-{stamp}-{seq:06}.plist")),
        )?;

        let mut backups: Vec<PathBuf> = std::fs::read_dir(&self.backups_dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(&format!("{label}-")))
            })
            .collect();
        backups.sort();
        while backups.len() > MAX_BACKUPS_PER_LABEL {
            let oldest = backups.remove(0);
            let _ = std::fs::remove_file(oldest);
        }
        Ok(())
    }

    fn write_plist(&self, path: &Path, value: &plist::Value) -> Result<(), BackendError> {
        value
            .to_file_xml(path)
            .map_err(|e| BackendError::Io(e.to_string()))
    }
}

fn label_of(value: &plist::Value) -> Option<String> {
    value
        .as_dictionary()?
        .get("Label")?
        .as_string()
        .map(str::to_string)
}

/// Commande affichable (§3.1) : le wrapper shell est déplié, sinon argv joint.
fn command_display(dict: &plist::Dictionary) -> String {
    if let Some(args) = dict.get("ProgramArguments").and_then(|v| v.as_array()) {
        let argv: Vec<&str> = args.iter().filter_map(|v| v.as_string()).collect();
        if argv.len() == 3
            && matches!(argv[0], "/bin/sh" | "/bin/bash" | "/bin/zsh")
            && argv[1] == "-c"
        {
            return argv[2].to_string();
        }
        if !argv.is_empty() {
            return argv.join(" ");
        }
    }
    dict.get("Program")
        .and_then(|v| v.as_string())
        .unwrap_or("")
        .to_string()
}

/// Reconstruit la spec éditable depuis le plist. None si l'agent n'a pas de
/// schedule (§DECISIONS) : le formulaire ne doit pas lui en ajouter un.
fn spec_of(dict: &plist::Dictionary, label: &str, schedule: &ScheduleInfo) -> Option<LaunchdJobSpec> {
    let spec_schedule = match schedule {
        ScheduleInfo::CalendarIntervals(entries) => {
            LaunchdSchedule::CalendarIntervals(entries.clone())
        }
        ScheduleInfo::Interval(secs) => LaunchdSchedule::Interval(*secs),
        _ => return None,
    };

    let command = match dict.get("ProgramArguments").and_then(|v| v.as_array()) {
        Some(args) => {
            let argv: Vec<String> = args
                .iter()
                .filter_map(|v| v.as_string().map(str::to_string))
                .collect();
            if argv.len() == 3
                && matches!(argv[0].as_str(), "/bin/sh" | "/bin/bash" | "/bin/zsh")
                && argv[1] == "-c"
            {
                LaunchdCommand::ShellWrapper(argv[2].clone())
            } else {
                LaunchdCommand::Argv(argv)
            }
        }
        None => LaunchdCommand::Argv(
            dict.get("Program")
                .and_then(|v| v.as_string())
                .map(|p| vec![p.to_string()])
                .unwrap_or_default(),
        ),
    };

    let path_of = |key: &str| {
        dict.get(key)
            .and_then(|v| v.as_string())
            .map(str::to_string)
    };

    Some(LaunchdJobSpec {
        label: label.to_string(),
        command,
        schedule: spec_schedule,
        run_at_load: dict
            .get("RunAtLoad")
            .and_then(|v| v.as_boolean())
            .unwrap_or(false),
        stdout_path: path_of("StandardOutPath"),
        stderr_path: path_of("StandardErrorPath"),
    })
}

fn schedule_of(dict: &plist::Dictionary) -> (ScheduleInfo, String) {
    if let Some(value) = dict.get("StartCalendarInterval") {
        if let Some(entries) = calendar::entries_from_plist(value) {
            let raw = calendar::summary(&entries);
            return (ScheduleInfo::CalendarIntervals(entries), raw);
        }
    }
    if let Some(secs) = dict
        .get("StartInterval")
        .and_then(|v| v.as_signed_integer())
        .and_then(|n| u32::try_from(n).ok())
    {
        return (ScheduleInfo::Interval(secs), format!("StartInterval={secs}s"));
    }
    (ScheduleInfo::None, "—".to_string())
}

/// Applique les clés gérées (§6.3) sur le dictionnaire EXISTANT : les clés
/// inconnues (KeepAlive, EnvironmentVariables...) sont préservées telles
/// quelles — jamais de désérialisation vers une struct fermée.
fn apply_spec(dict: &mut plist::Dictionary, spec: &LaunchdJobSpec) {
    dict.insert(
        "Label".to_string(),
        plist::Value::String(spec.label.clone()),
    );

    let argv: Vec<plist::Value> = match &spec.command {
        LaunchdCommand::ShellWrapper(cmd) => ["/bin/sh", "-c", cmd.as_str()]
            .iter()
            .map(|s| plist::Value::String((*s).to_string()))
            .collect(),
        LaunchdCommand::Argv(argv) => argv
            .iter()
            .map(|s| plist::Value::String(s.clone()))
            .collect(),
    };
    dict.insert("ProgramArguments".to_string(), plist::Value::Array(argv));
    // ProgramArguments fait foi : un Program hérité deviendrait ambigu.
    dict.remove("Program");

    dict.remove("StartCalendarInterval");
    dict.remove("StartInterval");
    match &spec.schedule {
        LaunchdSchedule::CalendarIntervals(entries) => {
            dict.insert(
                "StartCalendarInterval".to_string(),
                calendar::entries_to_plist(entries),
            );
        }
        LaunchdSchedule::Interval(secs) => {
            dict.insert(
                "StartInterval".to_string(),
                plist::Value::Integer(i64::from(*secs).into()),
            );
        }
    }

    if spec.run_at_load {
        dict.insert("RunAtLoad".to_string(), plist::Value::Boolean(true));
    } else {
        dict.remove("RunAtLoad");
    }

    for (key, value) in [
        ("StandardOutPath", &spec.stdout_path),
        ("StandardErrorPath", &spec.stderr_path),
    ] {
        match value {
            Some(path) if !path.trim().is_empty() => {
                dict.insert(key.to_string(), plist::Value::String(path.trim().to_string()));
            }
            _ => {
                dict.remove(key);
            }
        }
    }
}

fn validate_spec(spec: &LaunchdJobSpec) -> Result<(), BackendError> {
    let label = spec.label.trim();
    if label.is_empty()
        || label.contains('/')
        || label.contains("..")
        || label.chars().any(char::is_whitespace)
    {
        return Err(BackendError::InvalidSpec("label invalide".to_string()));
    }
    match &spec.command {
        LaunchdCommand::ShellWrapper(cmd) if cmd.trim().is_empty() => {
            return Err(BackendError::InvalidSpec("commande vide".to_string()));
        }
        LaunchdCommand::Argv(argv) if argv.is_empty() || argv[0].trim().is_empty() => {
            return Err(BackendError::InvalidSpec("argv vide".to_string()));
        }
        _ => {}
    }
    match &spec.schedule {
        LaunchdSchedule::CalendarIntervals(entries) if entries.is_empty() => {
            return Err(BackendError::InvalidSpec(
                "aucune entrée calendar".to_string(),
            ));
        }
        LaunchdSchedule::Interval(0) => {
            return Err(BackendError::InvalidSpec("intervalle nul".to_string()));
        }
        _ => {}
    }
    Ok(())
}

impl SchedulerBackend for LaunchdBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::Launchd
    }

    fn list(&self) -> Result<Vec<Job>, BackendError> {
        let (parsed, _) = self.scan()?;
        let disabled = self.disabled_labels();
        Ok(parsed
            .iter()
            .filter_map(|(_, v)| self.job_from_plist(v, &disabled))
            .collect())
    }

    fn get(&self, id: &JobId) -> Result<Job, BackendError> {
        let (_, value) = self.find_by_label(id)?;
        let disabled = self.disabled_labels();
        self.job_from_plist(&value, &disabled)
            .ok_or(BackendError::NotFound)
    }

    /// create (§6.4) : collision → erreur ; écrire, puis enable + bootstrap
    /// (l'enable purge une éventuelle entrée override désactivée résiduelle).
    fn create(&self, spec: &JobSpec) -> Result<JobId, BackendError> {
        let spec = Self::launchd_spec(spec)?;
        validate_spec(spec)?;
        let label = spec.label.trim();

        let path = self.plist_path(label);
        if path.exists()
            || self.find_by_label(label).is_ok()
            || self.print_status(label).loaded
        {
            return Err(BackendError::InvalidSpec(format!(
                "un agent « {label} » existe déjà"
            )));
        }

        let mut dict = plist::Dictionary::new();
        apply_spec(&mut dict, spec);
        std::fs::create_dir_all(&self.agents_dir)?;
        self.write_plist(&path, &plist::Value::Dictionary(dict))?;

        self.launchctl_ok(&["enable", &self.target(label)])?;
        self.launchctl_ok(&["bootstrap", &self.domain(), &path.to_string_lossy()])?;
        Ok(label.to_string())
    }

    /// update (§6.4) : backup → bootout → réécrire (clés inconnues préservées)
    /// → enable + bootstrap seulement s'il était chargé. Le garde-fou « job
    /// running » est côté UI (confirmation explicite).
    fn update(&self, id: &JobId, spec: &JobSpec) -> Result<(), BackendError> {
        let spec = Self::launchd_spec(spec)?;
        validate_spec(spec)?;
        if spec.label.trim() != id {
            return Err(BackendError::InvalidSpec(
                "changement de label non supporté — supprimez puis recréez".to_string(),
            ));
        }

        let (path, value) = self.find_by_label(id)?;
        let was_loaded = self.print_status(id).loaded;

        self.backup_plist(id, &path)?;
        if was_loaded {
            self.bootout_tolerant(id)?;
        }

        let mut dict = value.as_dictionary().cloned().unwrap_or_default();
        apply_spec(&mut dict, spec);
        self.write_plist(&path, &plist::Value::Dictionary(dict))?;

        if was_loaded {
            self.launchctl_ok(&["enable", &self.target(id)])?;
            self.launchctl_ok(&["bootstrap", &self.domain(), &path.to_string_lossy()])?;
        }
        Ok(())
    }

    /// delete (§6.4) : bootout toléré puis CORBEILLE — jamais de rm définitif.
    fn delete(&self, id: &JobId) -> Result<(), BackendError> {
        let (path, _) = self.find_by_label(id)?;
        self.bootout_tolerant(id)?;

        std::fs::create_dir_all(&self.trash_dir)?;
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("{id}.plist"));
        std::fs::rename(&path, self.trash_dir.join(format!("{stamp}-{file_name}")))?;
        Ok(())
    }

    /// enable/disable via l'override DB uniquement (§6.5) — on n'écrit JAMAIS
    /// la clé Disabled (primée par la DB).
    fn set_enabled(&self, id: &JobId, enabled: bool) -> Result<(), BackendError> {
        let (path, _) = self.find_by_label(id)?;
        if enabled {
            self.launchctl_ok(&["enable", &self.target(id)])?;
            if !self.print_status(id).loaded {
                self.launchctl_ok(&["bootstrap", &self.domain(), &path.to_string_lossy()])?;
            }
        } else {
            self.bootout_tolerant(id)?;
            self.launchctl_ok(&["disable", &self.target(id)])?;
        }
        Ok(())
    }

    /// run now (§6.6) : kickstart, asynchrone. Service non chargé → l'UI
    /// propose d'abord l'activation (statut connu avant l'appel).
    fn run_now(&self, id: &JobId) -> Result<RunResult, BackendError> {
        self.find_by_label(id)?;
        self.launchctl_ok(&["kickstart", &self.target(id)])?;
        Ok(RunResult::Started)
    }

    fn diagnostics(&self) -> Vec<Diagnostic> {
        let mut diags = Vec::new();

        diags.push(match self.scan() {
            Ok((parsed, unreadable)) => {
                if unreadable.is_empty() {
                    Diagnostic {
                        severity: DiagnosticSeverity::Ok,
                        code: "launchd.agentsDirAccessible".to_string(),
                        detail: Some(format!("{} plists", parsed.len())),
                    }
                } else {
                    Diagnostic {
                        severity: DiagnosticSeverity::Warning,
                        code: "launchd.unreadablePlists".to_string(),
                        detail: Some(unreadable.join(", ")),
                    }
                }
            }
            Err(e) => Diagnostic {
                severity: DiagnosticSeverity::Error,
                code: "launchd.agentsDirInaccessible".to_string(),
                detail: Some(e.to_string()),
            },
        });

        diags.push(
            match self.launchctl(&["print-disabled", &self.domain()]) {
                Ok((0, _, _)) => Diagnostic {
                    severity: DiagnosticSeverity::Ok,
                    code: "launchd.launchctlResponds".to_string(),
                    detail: None,
                },
                Ok((_, _, stderr)) => Diagnostic {
                    severity: DiagnosticSeverity::Error,
                    code: "launchd.launchctlFails".to_string(),
                    detail: Some(stderr),
                },
                Err(e) => Diagnostic {
                    severity: DiagnosticSeverity::Error,
                    code: "launchd.launchctlFails".to_string(),
                    detail: Some(e.to_string()),
                },
            },
        );

        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CalendarEntry;
    use crate::system::mock::MockSystem;

    /// Sortie print « chargé » du spike (état + last exit code).
    const PRINT_LOADED: &str =
        "\tstate = not running\n\tlast exit code = 3\n\tdomain = {\n\t\tstate = active\n\t}\n";
    const PRINT_NOT_LOADED: (i32, &str, &str) =
        (113, "", "Could not find service \"x\" in domain for user gui: 501");
    const BOOTOUT_NOT_LOADED: (i32, &str, &str) = (3, "", "Boot-out failed: 3: No such process");

    const UBERCRON_PLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>com.ubercron.test</string>
  <key>ProgramArguments</key><array>
    <string>/bin/sh</string><string>-c</string><string>/usr/local/bin/backup --full</string>
  </array>
  <key>StartCalendarInterval</key><dict>
    <key>Minute</key><integer>0</integer>
    <key>Hour</key><integer>4</integer>
  </dict>
  <key>StandardOutPath</key><string>/tmp/backup.log</string>
</dict></plist>
"#;

    const ADOBE_PLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>com.adobe.foo</string>
  <key>ProgramArguments</key><array>
    <string>/Applications/Adobe/foo</string><string>--daemon</string>
  </array>
  <key>KeepAlive</key><true/>
  <key>EnvironmentVariables</key><dict>
    <key>ADOBE_HOME</key><string>/Applications/Adobe</string>
  </dict>
</dict></plist>
"#;

    struct Fixture {
        system: Arc<MockSystem>,
        backend: LaunchdBackend,
        _dir: tempfile::TempDir,
        agents: PathBuf,
        trash: PathBuf,
        backups: PathBuf,
    }

    fn fixture(files: &[(&str, &str)]) -> Fixture {
        let system = Arc::new(MockSystem::new());
        let dir = tempfile::tempdir().unwrap();
        let agents = dir.path().join("LaunchAgents");
        let backups = dir.path().join("backups");
        let trash = dir.path().join("trash");
        std::fs::create_dir_all(&agents).unwrap();
        for (name, content) in files {
            std::fs::write(agents.join(name), content).unwrap();
        }
        let backend = LaunchdBackend::new(
            system.clone(),
            agents.clone(),
            backups.clone(),
            trash.clone(),
            501,
        );
        Fixture {
            system,
            backend,
            _dir: dir,
            agents,
            trash,
            backups,
        }
    }

    fn spec(label: &str) -> JobSpec {
        JobSpec::Launchd(LaunchdJobSpec {
            label: label.to_string(),
            command: LaunchdCommand::ShellWrapper("/usr/local/bin/tache".to_string()),
            schedule: LaunchdSchedule::CalendarIntervals(vec![CalendarEntry {
                minute: Some(30),
                hour: Some(7),
                day: None,
                weekday: Some(1),
                month: None,
            }]),
            run_at_load: false,
            stdout_path: Some("/tmp/tache.log".to_string()),
            stderr_path: None,
        })
    }

    fn subcommands(system: &MockSystem) -> Vec<String> {
        system
            .calls()
            .iter()
            .map(|c| c.args.first().cloned().unwrap_or_default())
            .collect()
    }

    #[test]
    fn list_lit_les_plists_generiques() {
        let f = fixture(&[
            ("com.adobe.foo.plist", ADOBE_PLIST),
            ("com.ubercron.test.plist", UBERCRON_PLIST),
        ]);
        // print-disabled : adobe désactivé dans l'override DB.
        f.system
            .push_response(0, "\t\"com.adobe.foo\" => disabled\n", "");
        // print par agent, ordre trié par nom de fichier : adobe puis ubercron.
        f.system
            .push_response(PRINT_NOT_LOADED.0, PRINT_NOT_LOADED.1, PRINT_NOT_LOADED.2);
        f.system.push_response(0, PRINT_LOADED, "");

        let jobs = f.backend.list().unwrap();
        assert_eq!(jobs.len(), 2);

        let adobe = &jobs[0];
        assert_eq!(adobe.label, "com.adobe.foo");
        assert!(!adobe.managed);
        assert!(!adobe.enabled);
        assert_eq!(adobe.status, JobStatus::NotLoaded);
        assert_eq!(adobe.command, "/Applications/Adobe/foo --daemon");
        assert!(matches!(adobe.schedule, ScheduleInfo::None));
        assert!(adobe.next_runs.is_empty());

        let ours = &jobs[1];
        assert_eq!(ours.label, "com.ubercron.test");
        assert!(ours.managed);
        assert!(ours.enabled);
        assert_eq!(ours.status, JobStatus::Loaded);
        assert_eq!(ours.last_exit_code, Some(3));
        assert_eq!(ours.command, "/usr/local/bin/backup --full");
        assert_eq!(ours.schedule_raw, "Minute=0 Hour=4");
        assert_eq!(ours.next_runs.len(), 5);
    }

    #[test]
    fn create_enable_avant_bootstrap() {
        let f = fixture(&[]);
        f.system
            .push_response(PRINT_NOT_LOADED.0, PRINT_NOT_LOADED.1, PRINT_NOT_LOADED.2);
        f.system.push_response(0, "", ""); // enable
        f.system.push_response(0, "", ""); // bootstrap

        let id = f.backend.create(&spec("com.ubercron.nouveau")).unwrap();
        assert_eq!(id, "com.ubercron.nouveau");
        assert_eq!(subcommands(&f.system), ["print", "enable", "bootstrap"]);
        assert!(f.agents.join("com.ubercron.nouveau.plist").exists());

        let value =
            plist::Value::from_file(f.agents.join("com.ubercron.nouveau.plist")).unwrap();
        let dict = value.as_dictionary().unwrap();
        assert!(dict.get("StartCalendarInterval").is_some());
        assert!(dict.get("RunAtLoad").is_none());
    }

    #[test]
    fn create_refuse_la_collision() {
        let f = fixture(&[("com.ubercron.test.plist", UBERCRON_PLIST)]);
        let err = f.backend.create(&spec("com.ubercron.test")).unwrap_err();
        assert!(matches!(err, BackendError::InvalidSpec(_)));
        assert!(f.system.calls().is_empty(), "aucun appel launchctl attendu");
    }

    #[test]
    fn update_preserve_les_cles_inconnues_et_fait_un_backup() {
        let f = fixture(&[("com.adobe.foo.plist", ADOBE_PLIST)]);
        f.system.push_response(0, PRINT_LOADED, ""); // print : chargé
        f.system.push_response(0, "", ""); // bootout
        f.system.push_response(0, "", ""); // enable
        f.system.push_response(0, "", ""); // bootstrap

        let mut s = spec("com.adobe.foo");
        if let JobSpec::Launchd(inner) = &mut s {
            inner.schedule = LaunchdSchedule::Interval(3600);
        }
        f.backend.update(&"com.adobe.foo".to_string(), &s).unwrap();

        assert_eq!(
            subcommands(&f.system),
            ["print", "bootout", "enable", "bootstrap"]
        );

        let value = plist::Value::from_file(f.agents.join("com.adobe.foo.plist")).unwrap();
        let dict = value.as_dictionary().unwrap();
        // Clés inconnues intactes (§6.3).
        assert!(dict.get("KeepAlive").is_some());
        assert!(dict.get("EnvironmentVariables").is_some());
        // Clés gérées mises à jour.
        assert_eq!(
            dict.get("StartInterval").and_then(|v| v.as_signed_integer()),
            Some(3600)
        );
        assert!(dict.get("StartCalendarInterval").is_none());

        let backups: Vec<_> = std::fs::read_dir(&f.backups).unwrap().collect();
        assert_eq!(backups.len(), 1);
    }

    #[test]
    fn update_non_charge_ecrit_seulement() {
        let f = fixture(&[("com.ubercron.test.plist", UBERCRON_PLIST)]);
        f.system
            .push_response(PRINT_NOT_LOADED.0, PRINT_NOT_LOADED.1, PRINT_NOT_LOADED.2);

        f.backend
            .update(&"com.ubercron.test".to_string(), &spec("com.ubercron.test"))
            .unwrap();
        assert_eq!(subcommands(&f.system), ["print"], "ni bootout ni bootstrap");
    }

    #[test]
    fn update_refuse_le_changement_de_label() {
        let f = fixture(&[("com.ubercron.test.plist", UBERCRON_PLIST)]);
        let err = f
            .backend
            .update(&"com.ubercron.test".to_string(), &spec("com.ubercron.autre"))
            .unwrap_err();
        assert!(matches!(err, BackendError::InvalidSpec(_)));
    }

    #[test]
    fn delete_deplace_en_corbeille_meme_non_charge() {
        let f = fixture(&[("com.ubercron.test.plist", UBERCRON_PLIST)]);
        f.system
            .push_response(BOOTOUT_NOT_LOADED.0, BOOTOUT_NOT_LOADED.1, BOOTOUT_NOT_LOADED.2);

        f.backend.delete(&"com.ubercron.test".to_string()).unwrap();
        assert!(!f.agents.join("com.ubercron.test.plist").exists());
        let trashed: Vec<String> = std::fs::read_dir(&f.trash)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(trashed.len(), 1);
        assert!(trashed[0].ends_with("com.ubercron.test.plist"));
    }

    #[test]
    fn enable_sequence_enable_puis_bootstrap_si_non_charge() {
        let f = fixture(&[("com.ubercron.test.plist", UBERCRON_PLIST)]);
        f.system.push_response(0, "", ""); // enable
        f.system
            .push_response(PRINT_NOT_LOADED.0, PRINT_NOT_LOADED.1, PRINT_NOT_LOADED.2);
        f.system.push_response(0, "", ""); // bootstrap

        f.backend
            .set_enabled(&"com.ubercron.test".to_string(), true)
            .unwrap();
        assert_eq!(subcommands(&f.system), ["enable", "print", "bootstrap"]);
    }

    #[test]
    fn disable_tolere_le_bootout_deja_decharge() {
        let f = fixture(&[("com.ubercron.test.plist", UBERCRON_PLIST)]);
        f.system
            .push_response(BOOTOUT_NOT_LOADED.0, BOOTOUT_NOT_LOADED.1, BOOTOUT_NOT_LOADED.2);
        f.system.push_response(0, "", ""); // disable

        f.backend
            .set_enabled(&"com.ubercron.test".to_string(), false)
            .unwrap();
        assert_eq!(subcommands(&f.system), ["bootout", "disable"]);
    }

    #[test]
    fn run_now_kickstart() {
        let f = fixture(&[("com.ubercron.test.plist", UBERCRON_PLIST)]);
        f.system.push_response(0, "", "");
        let r = f.backend.run_now(&"com.ubercron.test".to_string()).unwrap();
        assert!(matches!(r, RunResult::Started));

        f.system.push_response(
            PRINT_NOT_LOADED.0,
            PRINT_NOT_LOADED.1,
            PRINT_NOT_LOADED.2,
        );
        let err = f
            .backend
            .run_now(&"com.ubercron.test".to_string())
            .unwrap_err();
        assert!(matches!(err, BackendError::CommandFailed { .. }));
    }

    #[test]
    fn specs_invalides_rejetees() {
        let f = fixture(&[]);
        let mut s = spec("label avec espaces");
        assert!(matches!(
            f.backend.create(&s),
            Err(BackendError::InvalidSpec(_))
        ));
        s = spec("com.ubercron.ok");
        if let JobSpec::Launchd(inner) = &mut s {
            inner.schedule = LaunchdSchedule::Interval(0);
        }
        assert!(matches!(
            f.backend.create(&s),
            Err(BackendError::InvalidSpec(_))
        ));
    }
}
