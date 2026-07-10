//! Parsing défensif des sorties de launchctl (§6.2). Format non garanti par
//! Apple : tout échec de parsing dégrade en Unknown, jamais en erreur.
//! Les formats attendus sont documentés dans DECISIONS.md (spike 2026-07-10).

use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintStatus {
    pub loaded: bool,
    /// None si la ligne `state = ...` est introuvable (format inattendu).
    pub running: Option<bool>,
    pub last_exit_code: Option<i32>,
}

/// `launchctl print gui/$UID/<label>` : exit 0 = chargé, 113 = non chargé.
/// Piège (spike) : des `state = active` existent dans des sous-sections plus
/// profondes — seule la PREMIÈRE occurrence fait foi.
pub fn parse_print(status: i32, stdout: &str) -> PrintStatus {
    if status != 0 {
        return PrintStatus {
            loaded: false,
            running: None,
            last_exit_code: None,
        };
    }
    let mut running = None;
    let mut last_exit_code = None;
    for line in stdout.lines() {
        let line = line.trim();
        if running.is_none() {
            if let Some(v) = line.strip_prefix("state = ") {
                running = Some(v.trim() == "running");
            }
        }
        if last_exit_code.is_none() {
            if let Some(v) = line.strip_prefix("last exit code = ") {
                // "(never exited)" → None, valeur numérique → Some.
                last_exit_code = v.trim().parse::<i32>().ok();
            }
        }
    }
    PrintStatus {
        loaded: true,
        running,
        last_exit_code,
    }
}

/// `launchctl print-disabled gui/$UID` : lignes `"label" => disabled`.
/// Après un enable, l'entrée RESTE présente en `=> enabled` (spike) — seul
/// `=> disabled` (ou `=> true`, format historique) compte.
pub fn parse_print_disabled(stdout: &str) -> HashSet<String> {
    stdout
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let rest = line.strip_prefix('"')?;
            let (label, tail) = rest.split_once('"')?;
            let value = tail.trim().strip_prefix("=>")?.trim();
            (value == "disabled" || value == "true").then(|| label.to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sortie réelle du spike (élaguée), avec les state des sous-sections.
    const PRINT_LOADED: &str = "\
com.ubercron.spike = {
	path = /Users/moi/Library/LaunchAgents/com.ubercron.spike.plist
	state = not running
	program = /bin/sh
	last exit code = 3
	spawn type = daemon (3)
	domain = {
		state = active
	}
	asid = {
		state = active
	}
}";

    #[test]
    fn print_charge_prend_le_premier_state() {
        let s = parse_print(0, PRINT_LOADED);
        assert!(s.loaded);
        assert_eq!(s.running, Some(false));
        assert_eq!(s.last_exit_code, Some(3));
    }

    #[test]
    fn print_never_exited_donne_none() {
        let out = "\tstate = running\n\tlast exit code = (never exited)\n";
        let s = parse_print(0, out);
        assert_eq!(s.running, Some(true));
        assert_eq!(s.last_exit_code, None);
    }

    #[test]
    fn print_non_charge_exit_113() {
        let s = parse_print(113, "");
        assert!(!s.loaded);
        assert_eq!(s.running, None);
    }

    #[test]
    fn print_format_inattendu_degrade_en_unknown() {
        let s = parse_print(0, "quelque chose de nouveau chez Apple");
        assert!(s.loaded);
        assert_eq!(s.running, None);
        assert_eq!(s.last_exit_code, None);
    }

    #[test]
    fn print_disabled_ne_retient_que_disabled() {
        let out = "disabled services = {\n\t\"com.ubercron.spike\" => enabled\n\t\"com.adobe.truc\" => disabled\n\t\"com.vieux.format\" => true\n}\n";
        let set = parse_print_disabled(out);
        assert!(!set.contains("com.ubercron.spike"));
        assert!(set.contains("com.adobe.truc"));
        assert!(set.contains("com.vieux.format"));
    }
}
