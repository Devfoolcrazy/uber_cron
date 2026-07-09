//! Parser crontab lossless (§5.1).
//!
//! Invariant : `Crontab::parse(text).serialize() == text` pour tout texte non modifié.
//! On ne reformate JAMAIS une ligne qu'on n'a pas éditée : chaque ligne conserve son
//! `raw` d'origine, réémis tel quel à la sérialisation.

/// Marqueur de désactivation (§5.3). Les jobs commentés à la main restent des Comment.
pub const DISABLED_MARKER: &str = "# UBERCRON-DISABLED: ";

const SHORTCUTS: [&str; 8] = [
    "@reboot", "@yearly", "@annually", "@monthly", "@weekly", "@daily", "@midnight", "@hourly",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrontabLine {
    Job {
        schedule: String,
        command: String,
        /// Nom issu du commentaire `# name: ...` précédant immédiatement la ligne.
        name: Option<String>,
        /// Ligne de commentaire nom d'origine, réémise telle quelle (lossless).
        name_raw: Option<String>,
        raw: String,
    },
    DisabledJob {
        schedule: String,
        command: String,
        name: Option<String>,
        name_raw: Option<String>,
        raw: String,
    },
    EnvVar {
        key: String,
        value: String,
        raw: String,
    },
    Comment {
        raw: String,
    },
    Blank {
        raw: String,
    },
    /// Ligne non analysable : PRÉSERVÉE telle quelle, jamais réécrite.
    Unknown {
        raw: String,
    },
}

impl CrontabLine {
    pub fn raw_lines(&self) -> Vec<&str> {
        match self {
            CrontabLine::Job { name_raw, raw, .. }
            | CrontabLine::DisabledJob { name_raw, raw, .. } => match name_raw {
                Some(n) => vec![n.as_str(), raw.as_str()],
                None => vec![raw.as_str()],
            },
            CrontabLine::EnvVar { raw, .. }
            | CrontabLine::Comment { raw }
            | CrontabLine::Blank { raw }
            | CrontabLine::Unknown { raw } => vec![raw.as_str()],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crontab {
    pub lines: Vec<CrontabLine>,
    /// Le texte d'origine se terminait-il par '\n' ? (round-trip exact)
    trailing_newline: bool,
}

impl Crontab {
    pub fn parse(text: &str) -> Crontab {
        let trailing_newline = text.ends_with('\n');
        let body = text.strip_suffix('\n').unwrap_or(text);

        let mut lines: Vec<CrontabLine> = Vec::new();
        if !body.is_empty() || (text == "\n") {
            for raw in body.split('\n') {
                lines.push(classify(raw));
            }
        }

        // Deuxième passe : associer les commentaires `# name: ...` au job qui suit (§5.1).
        let mut merged: Vec<CrontabLine> = Vec::with_capacity(lines.len());
        let mut iter = lines.into_iter().peekable();
        while let Some(line) = iter.next() {
            let name_info = match &line {
                CrontabLine::Comment { raw } => parse_name_comment(raw).map(|n| (n, raw.clone())),
                _ => None,
            };
            if let Some((name, name_raw)) = name_info {
                if matches!(
                    iter.peek(),
                    Some(CrontabLine::Job { .. }) | Some(CrontabLine::DisabledJob { .. })
                ) {
                    match iter.next() {
                        Some(CrontabLine::Job {
                            schedule,
                            command,
                            raw,
                            ..
                        }) => {
                            merged.push(CrontabLine::Job {
                                schedule,
                                command,
                                name: Some(name),
                                name_raw: Some(name_raw),
                                raw,
                            });
                            continue;
                        }
                        Some(CrontabLine::DisabledJob {
                            schedule,
                            command,
                            raw,
                            ..
                        }) => {
                            merged.push(CrontabLine::DisabledJob {
                                schedule,
                                command,
                                name: Some(name),
                                name_raw: Some(name_raw),
                                raw,
                            });
                            continue;
                        }
                        _ => unreachable!("peek garantit un job"),
                    }
                }
            }
            merged.push(line);
        }

        Crontab {
            lines: merged,
            trailing_newline,
        }
    }

    pub fn serialize(&self) -> String {
        let mut out: Vec<&str> = Vec::new();
        for line in &self.lines {
            out.extend(line.raw_lines());
        }
        let mut text = out.join("\n");
        if self.trailing_newline {
            text.push('\n');
        }
        text
    }
}

fn classify(raw: &str) -> CrontabLine {
    let raw_string = raw.to_string();
    let trimmed = raw.trim();

    if trimmed.is_empty() {
        return CrontabLine::Blank { raw: raw_string };
    }

    // Marqueur UBERCRON-DISABLED : ce qui suit doit être une ligne job valide,
    // sinon on retombe en Comment (on ne devine pas, §5.3).
    if let Some(rest) = raw.strip_prefix(DISABLED_MARKER) {
        if let Some((schedule, command)) = parse_job_line(rest) {
            return CrontabLine::DisabledJob {
                schedule,
                command,
                name: None,
                name_raw: None,
                raw: raw_string,
            };
        }
        return CrontabLine::Comment { raw: raw_string };
    }

    if trimmed.starts_with('#') {
        return CrontabLine::Comment { raw: raw_string };
    }

    if let Some((key, value)) = parse_env_var(raw) {
        return CrontabLine::EnvVar {
            key,
            value,
            raw: raw_string,
        };
    }

    if let Some((schedule, command)) = parse_job_line(raw) {
        return CrontabLine::Job {
            schedule,
            command,
            name: None,
            name_raw: None,
            raw: raw_string,
        };
    }

    CrontabLine::Unknown { raw: raw_string }
}

/// `# name: Mon backup` → Some("Mon backup")
fn parse_name_comment(raw: &str) -> Option<String> {
    let rest = raw.trim_start().strip_prefix('#')?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix("name:")?;
    let name = rest.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// `KEY = value` (Vixie accepte les espaces autour de `=`).
fn parse_env_var(raw: &str) -> Option<(String, String)> {
    let trimmed = raw.trim_start();
    let eq = trimmed.find('=')?;
    let key = trimmed[..eq].trim_end();
    if key.is_empty()
        || !key
            .chars()
            .next()
            .map(|c| c.is_ascii_alphabetic() || c == '_')
            .unwrap_or(false)
        || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return None;
    }
    let value = trimmed[eq + 1..].trim().to_string();
    Some((key.to_string(), value))
}

/// Ligne job : `@raccourci commande` ou `m h dom mon dow commande`.
/// Retourne (schedule, commande) en préservant l'espacement interne de la commande.
fn parse_job_line(raw: &str) -> Option<(String, String)> {
    let trimmed = raw.trim_start();

    if trimmed.starts_with('@') {
        let (token, rest) = split_first_token(trimmed);
        if SHORTCUTS.contains(&token) && !rest.trim().is_empty() {
            return Some((token.to_string(), rest.trim_start().to_string()));
        }
        return None;
    }

    // 5 champs puis la commande. On avance token par token pour garder l'espacement
    // interne de la commande intact (lossless).
    let mut rest = trimmed;
    let mut fields: Vec<&str> = Vec::with_capacity(5);
    for _ in 0..5 {
        let (token, r) = split_first_token(rest);
        if token.is_empty() || !is_cron_field(token) {
            return None;
        }
        fields.push(token);
        rest = r.trim_start();
    }
    if rest.is_empty() {
        return None;
    }
    Some((fields.join(" "), rest.to_string()))
}

fn split_first_token(s: &str) -> (&str, &str) {
    match s.find(|c: char| c.is_ascii_whitespace()) {
        Some(i) => (&s[..i], &s[i..]),
        None => (s, ""),
    }
}

/// Charset plausible d'un champ cron (les noms jan-dec / mon-sun sont permis).
/// La validation sémantique fine est faite par croner au calcul des occurrences.
fn is_cron_field(token: &str) -> bool {
    !token.is_empty()
        && token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '*' | ',' | '-' | '/' | '?'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(line: &CrontabLine) -> (&str, &str, Option<&str>) {
        match line {
            CrontabLine::Job {
                schedule,
                command,
                name,
                ..
            } => (schedule.as_str(), command.as_str(), name.as_deref()),
            other => panic!("attendu Job, obtenu {other:?}"),
        }
    }

    #[test]
    fn classifie_job_simple() {
        let ct = Crontab::parse("0 9 * * 1 /usr/bin/backup.sh\n");
        assert_eq!(ct.lines.len(), 1);
        let (s, c, n) = job(&ct.lines[0]);
        assert_eq!(s, "0 9 * * 1");
        assert_eq!(c, "/usr/bin/backup.sh");
        assert_eq!(n, None);
    }

    #[test]
    fn preserve_espacement_commande() {
        let raw = "*/5\t*  * * *   echo  'deux  espaces'\n";
        let ct = Crontab::parse(raw);
        let (_, c, _) = job(&ct.lines[0]);
        assert_eq!(c, "echo  'deux  espaces'");
        assert_eq!(ct.serialize(), raw);
    }

    #[test]
    fn classifie_env_comment_blank_unknown() {
        let ct = Crontab::parse("SHELL=/bin/sh\nPATH = /usr/bin:/bin\n# commentaire\n\nn'importe quoi\n");
        assert!(matches!(&ct.lines[0], CrontabLine::EnvVar { key, value, .. } if key == "SHELL" && value == "/bin/sh"));
        assert!(matches!(&ct.lines[1], CrontabLine::EnvVar { key, .. } if key == "PATH"));
        assert!(matches!(&ct.lines[2], CrontabLine::Comment { .. }));
        assert!(matches!(&ct.lines[3], CrontabLine::Blank { .. }));
        assert!(matches!(&ct.lines[4], CrontabLine::Unknown { .. }));
    }

    #[test]
    fn raccourcis_at() {
        let ct = Crontab::parse("@daily /usr/local/bin/nettoyage\n@reboot /usr/local/bin/demarrage\n@bogus pas un job\n");
        let (s, _, _) = job(&ct.lines[0]);
        assert_eq!(s, "@daily");
        let (s, _, _) = job(&ct.lines[1]);
        assert_eq!(s, "@reboot");
        assert!(matches!(&ct.lines[2], CrontabLine::Unknown { .. }));
    }

    #[test]
    fn nom_associe_au_job_suivant() {
        let ct = Crontab::parse("# name: Mon backup\n0 3 * * * /usr/bin/backup.sh\n");
        assert_eq!(ct.lines.len(), 1);
        let (_, _, n) = job(&ct.lines[0]);
        assert_eq!(n, Some("Mon backup"));
    }

    #[test]
    fn nom_orphelin_reste_commentaire() {
        let ct = Crontab::parse("# name: Orphelin\n# autre commentaire\n");
        assert_eq!(ct.lines.len(), 2);
        assert!(matches!(&ct.lines[0], CrontabLine::Comment { .. }));
    }

    #[test]
    fn job_desactive_par_marqueur() {
        let ct = Crontab::parse("# UBERCRON-DISABLED: 0 9 * * 1 /usr/bin/backup.sh\n");
        assert!(matches!(
            &ct.lines[0],
            CrontabLine::DisabledJob { schedule, command, .. }
                if schedule == "0 9 * * 1" && command == "/usr/bin/backup.sh"
        ));
    }

    #[test]
    fn marqueur_sur_contenu_invalide_reste_commentaire() {
        let ct = Crontab::parse("# UBERCRON-DISABLED: pas un job\n");
        assert!(matches!(&ct.lines[0], CrontabLine::Comment { .. }));
    }

    #[test]
    fn commentaire_manuel_reste_commentaire() {
        // Un job commenté "à la main" ne devient pas un DisabledJob (§5.3).
        let ct = Crontab::parse("#0 9 * * 1 /usr/bin/backup.sh\n");
        assert!(matches!(&ct.lines[0], CrontabLine::Comment { .. }));
    }

    #[test]
    fn crontab_vide() {
        let ct = Crontab::parse("");
        assert!(ct.lines.is_empty());
        assert_eq!(ct.serialize(), "");
    }

    #[test]
    fn sans_newline_final() {
        let raw = "0 9 * * 1 /usr/bin/backup.sh";
        assert_eq!(Crontab::parse(raw).serialize(), raw);
    }

    #[test]
    fn cinq_champs_sans_commande_est_unknown() {
        let ct = Crontab::parse("0 9 * * 1\n");
        assert!(matches!(&ct.lines[0], CrontabLine::Unknown { .. }));
    }
}
