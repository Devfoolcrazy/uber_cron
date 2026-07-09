//! Prochaines occurrences d'une expression cron (§5.4), via croner.
//!
//! Secondes interdites : nos schedules sont strictement 5 champs (+ @-raccourcis).
//! `@reboot` n'a pas d'occurrences calculables (badge "au démarrage" côté UI).

use chrono::{DateTime, TimeZone};
use croner::parser::{CronParser, Seconds};
use croner::Cron;

fn parse(schedule: &str) -> Option<Cron> {
    let trimmed = schedule.trim();
    if trimmed.eq_ignore_ascii_case("@reboot") {
        return None;
    }
    // croner gère @daily/@hourly/@weekly/@monthly/@yearly/@annually, pas @midnight.
    let normalized = if trimmed.eq_ignore_ascii_case("@midnight") {
        "0 0 * * *"
    } else {
        trimmed
    };
    CronParser::builder()
        .seconds(Seconds::Disallowed)
        .build()
        .parse(normalized)
        .ok()
}

/// L'expression est-elle acceptée par croner (ou @reboot) ?
pub fn is_valid(schedule: &str) -> bool {
    schedule.trim().eq_ignore_ascii_case("@reboot") || parse(schedule).is_some()
}

/// Les `count` prochaines occurrences strictement après `from`.
/// Vide si non calculable (@reboot, expression invalide).
pub fn next_runs_after<Tz: TimeZone + Copy>(
    schedule: &str,
    count: usize,
    from: DateTime<Tz>,
) -> Vec<DateTime<Tz>> {
    match parse(schedule) {
        Some(cron) => cron.iter_after(from).take(count).collect(),
        None => Vec::new(),
    }
}

/// Variante RFC3339 en timezone locale, prête pour l'IPC (§3.1).
pub fn next_runs_rfc3339(schedule: &str, count: usize) -> Vec<String> {
    next_runs_after(schedule, count, chrono::Local::now())
        .into_iter()
        .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    /// Mercredi 2026-01-07 12:30:00 +02:00 — timezone figée (§9).
    fn from() -> DateTime<FixedOffset> {
        FixedOffset::east_opt(2 * 3600)
            .unwrap()
            .with_ymd_and_hms(2026, 1, 7, 12, 30, 0)
            .unwrap()
    }

    fn runs(schedule: &str, count: usize) -> Vec<String> {
        next_runs_after(schedule, count, from())
            .iter()
            .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
            .collect()
    }

    #[test]
    fn hebdo_lundi_9h() {
        assert_eq!(
            runs("0 9 * * 1", 3),
            ["2026-01-12 09:00", "2026-01-19 09:00", "2026-01-26 09:00"]
        );
    }

    #[test]
    fn toutes_les_5_minutes() {
        assert_eq!(
            runs("*/5 * * * *", 3),
            ["2026-01-07 12:35", "2026-01-07 12:40", "2026-01-07 12:45"]
        );
    }

    #[test]
    fn premier_du_mois_minuit() {
        assert_eq!(
            runs("0 0 1 * *", 2),
            ["2026-02-01 00:00", "2026-03-01 00:00"]
        );
    }

    #[test]
    fn raccourci_daily_et_midnight() {
        assert_eq!(runs("@daily", 1), ["2026-01-08 00:00"]);
        assert_eq!(runs("@midnight", 1), ["2026-01-08 00:00"]);
    }

    #[test]
    fn reboot_sans_occurrences_mais_valide() {
        assert!(runs("@reboot", 5).is_empty());
        assert!(is_valid("@reboot"));
    }

    #[test]
    fn noms_de_jours_et_mois() {
        assert_eq!(runs("30 6 * jan mon", 1), ["2026-01-12 06:30"]);
    }

    #[test]
    fn invalides_rejetes() {
        assert!(!is_valid("99 9 * * *"));
        assert!(!is_valid("0 9 * * 1 extra"));
        assert!(!is_valid("n'importe quoi"));
        assert!(runs("99 9 * * *", 5).is_empty());
    }
}
