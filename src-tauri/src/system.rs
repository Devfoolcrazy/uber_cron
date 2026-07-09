//! Abstraction des appels système (§9) : `crontab`, `launchctl`, `/bin/sh` passent
//! par ce trait pour être mockés dans les tests — aucune écriture réelle dans `cargo test`.

use std::io::Write;
use std::process::{Command, Stdio};

#[derive(Debug, Clone)]
pub struct CmdOutput {
    /// Code de sortie (-1 si tué par signal).
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

pub trait SystemCommands: Send + Sync {
    /// Exécute `program args...`, en pipant `stdin` si fourni, et capture la sortie.
    fn run(&self, program: &str, args: &[&str], stdin: Option<&str>) -> std::io::Result<CmdOutput>;
}

/// Implémentation réelle. Chemins absolus recommandés par les appelants (§11 PATH).
pub struct RealSystem;

impl SystemCommands for RealSystem {
    fn run(&self, program: &str, args: &[&str], stdin: Option<&str>) -> std::io::Result<CmdOutput> {
        let mut cmd = Command::new(program);
        cmd.args(args)
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn()?;
        if let Some(input) = stdin {
            // L'enfant peut fermer stdin avant la fin de l'écriture : un EPIPE ne doit
            // pas masquer le vrai diagnostic (exit code + stderr), on l'ignore donc.
            if let Some(mut pipe) = child.stdin.take() {
                let _ = pipe.write_all(input.as_bytes());
            }
        }
        let out = child.wait_with_output()?;
        Ok(CmdOutput {
            status: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }
}

#[cfg(test)]
pub mod mock {
    use super::{CmdOutput, SystemCommands};
    use std::sync::Mutex;

    /// Un appel enregistré par le mock.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct RecordedCall {
        pub program: String,
        pub args: Vec<String>,
        pub stdin: Option<String>,
    }

    /// Mock scripté : les réponses sont consommées dans l'ordre (FIFO) ;
    /// chaque appel est enregistré pour les assertions.
    #[derive(Default)]
    pub struct MockSystem {
        responses: Mutex<std::collections::VecDeque<CmdOutput>>,
        pub calls: Mutex<Vec<RecordedCall>>,
    }

    impl MockSystem {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn push_response(&self, status: i32, stdout: &str, stderr: &str) {
            self.responses.lock().unwrap().push_back(CmdOutput {
                status,
                stdout: stdout.to_string(),
                stderr: stderr.to_string(),
            });
        }

        pub fn calls(&self) -> Vec<RecordedCall> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl SystemCommands for MockSystem {
        fn run(
            &self,
            program: &str,
            args: &[&str],
            stdin: Option<&str>,
        ) -> std::io::Result<CmdOutput> {
            self.calls.lock().unwrap().push(RecordedCall {
                program: program.to_string(),
                args: args.iter().map(|s| s.to_string()).collect(),
                stdin: stdin.map(|s| s.to_string()),
            });
            self.responses.lock().unwrap().pop_front().ok_or_else(|| {
                std::io::Error::other(format!("MockSystem : réponse non scriptée pour {program}"))
            })
        }
    }
}
