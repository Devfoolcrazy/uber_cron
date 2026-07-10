//! StartCalendarInterval : extraction depuis le plist et calcul des
//! prochaines occurrences (§6.7) — itération bornée à 366 jours.

use chrono::{DateTime, Datelike, Duration, TimeZone, Timelike};

use crate::model::CalendarEntry;

/// Extrait les entrées d'une valeur StartCalendarInterval : dict unique ou
/// array de dicts. None si la forme est inattendue.
pub fn entries_from_plist(value: &plist::Value) -> Option<Vec<CalendarEntry>> {
    match value {
        plist::Value::Dictionary(d) => Some(vec![entry_from_dict(d)]),
        plist::Value::Array(items) => {
            let entries: Vec<CalendarEntry> = items
                .iter()
                .filter_map(|v| v.as_dictionary().map(entry_from_dict))
                .collect();
            (!entries.is_empty()).then_some(entries)
        }
        _ => None,
    }
}

fn get_u8(d: &plist::Dictionary, key: &str) -> Option<u8> {
    d.get(key)
        .and_then(|v| v.as_signed_integer())
        .and_then(|n| u8::try_from(n).ok())
}

fn entry_from_dict(d: &plist::Dictionary) -> CalendarEntry {
    CalendarEntry {
        minute: get_u8(d, "Minute"),
        hour: get_u8(d, "Hour"),
        day: get_u8(d, "Day"),
        // 0 ET 7 = dimanche (§11) : normalisé à 0.
        weekday: get_u8(d, "Weekday").map(|w| if w == 7 { 0 } else { w }),
        month: get_u8(d, "Month"),
    }
}

/// Reconstruit la valeur plist (dict unique si une entrée, array sinon).
pub fn entries_to_plist(entries: &[CalendarEntry]) -> plist::Value {
    let dicts: Vec<plist::Value> = entries
        .iter()
        .map(|e| {
            let mut d = plist::Dictionary::new();
            let mut put = |key: &str, v: Option<u8>| {
                if let Some(n) = v {
                    d.insert(key.to_string(), plist::Value::Integer(i64::from(n).into()));
                }
            };
            put("Minute", e.minute);
            put("Hour", e.hour);
            put("Day", e.day);
            put("Weekday", e.weekday);
            put("Month", e.month);
            plist::Value::Dictionary(d)
        })
        .collect();
    match <[plist::Value; 1]>::try_from(dicts) {
        Ok([single]) => single,
        Err(dicts) => plist::Value::Array(dicts),
    }
}

fn day_may_match<Tz: TimeZone>(entry: &CalendarEntry, t: &DateTime<Tz>) -> bool {
    entry.month.is_none_or(|m| u32::from(m) == t.month())
        && entry.day.is_none_or(|d| u32::from(d) == t.day())
        && entry
            .weekday
            .is_none_or(|w| u32::from(w) == t.weekday().num_days_from_sunday())
}

fn time_matches<Tz: TimeZone>(entry: &CalendarEntry, t: &DateTime<Tz>) -> bool {
    entry.hour.is_none_or(|h| u32::from(h) == t.hour())
        && entry.minute.is_none_or(|m| u32::from(m) == t.minute())
}

/// Les `count` prochaines occurrences strictement après `from`, alignées à la
/// minute (launchd déclenche à la seconde 0). Vide si rien sous 366 jours.
pub fn next_runs<Tz: TimeZone + Copy>(
    entries: &[CalendarEntry],
    count: usize,
    from: DateTime<Tz>,
) -> Vec<DateTime<Tz>> {
    let mut results = Vec::new();
    if entries.is_empty() || count == 0 {
        return results;
    }
    // Prochaine minute pleine strictement après `from`.
    let mut t = from
        .with_second(0)
        .and_then(|t| t.with_nanosecond(0))
        .unwrap_or_else(|| from.clone())
        + Duration::minutes(1);
    let horizon = from.clone() + Duration::days(366);

    while t <= horizon && results.len() < count {
        let candidates: Vec<&CalendarEntry> =
            entries.iter().filter(|e| day_may_match(e, &t)).collect();
        if candidates.is_empty() {
            // Aucune entrée ne peut matcher ce jour : sauter au jour suivant.
            let next_day = (t.clone() + Duration::days(1))
                .with_hour(0)
                .and_then(|t| t.with_minute(0));
            t = next_day.unwrap_or_else(|| t + Duration::days(1));
            continue;
        }
        if candidates.iter().any(|e| time_matches(e, &t)) {
            results.push(t.clone());
        }
        t += Duration::minutes(1);
    }
    results
}

/// Variante RFC3339 en timezone locale, pour l'IPC.
pub fn next_runs_rfc3339(entries: &[CalendarEntry], count: usize) -> Vec<String> {
    next_runs(entries, count, chrono::Local::now())
        .into_iter()
        .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .collect()
}

/// Résumé technique pour `schedule_raw` (§3.1) — la phrase humaine est
/// générée côté frontend depuis les entrées structurées.
pub fn summary(entries: &[CalendarEntry]) -> String {
    if entries.len() > 1 {
        return format!("StartCalendarInterval ×{}", entries.len());
    }
    let e = &entries[0];
    let mut parts = Vec::new();
    let mut put = |name: &str, v: Option<u8>| {
        if let Some(n) = v {
            parts.push(format!("{name}={n}"));
        }
    };
    put("Minute", e.minute);
    put("Hour", e.hour);
    put("Day", e.day);
    put("Weekday", e.weekday);
    put("Month", e.month);
    if parts.is_empty() {
        "StartCalendarInterval {}".to_string()
    } else {
        parts.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    fn entry(
        minute: Option<u8>,
        hour: Option<u8>,
        day: Option<u8>,
        weekday: Option<u8>,
        month: Option<u8>,
    ) -> CalendarEntry {
        CalendarEntry {
            minute,
            hour,
            day,
            weekday,
            month,
        }
    }

    /// Mercredi 2026-01-07 12:30:00 +02:00 — timezone figée (§9).
    fn from() -> DateTime<FixedOffset> {
        FixedOffset::east_opt(2 * 3600)
            .unwrap()
            .with_ymd_and_hms(2026, 1, 7, 12, 30, 0)
            .unwrap()
    }

    fn runs(entries: &[CalendarEntry], count: usize) -> Vec<String> {
        next_runs(entries, count, from())
            .iter()
            .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
            .collect()
    }

    #[test]
    fn lundi_9h() {
        let e = [entry(Some(0), Some(9), None, Some(1), None)];
        assert_eq!(runs(&e, 2), ["2026-01-12 09:00", "2026-01-19 09:00"]);
    }

    #[test]
    fn weekday_7_equivaut_dimanche() {
        let d = {
            let mut d = plist::Dictionary::new();
            d.insert("Weekday".into(), plist::Value::Integer(7.into()));
            d.insert("Hour".into(), plist::Value::Integer(8.into()));
            d.insert("Minute".into(), plist::Value::Integer(0.into()));
            d
        };
        let entries =
            entries_from_plist(&plist::Value::Dictionary(d)).expect("dict valide");
        assert_eq!(entries[0].weekday, Some(0));
        assert_eq!(runs(&entries, 1), ["2026-01-11 08:00"]);
    }

    #[test]
    fn array_d_entrees_fusionne_les_occurrences() {
        // 08:00 et 20:00 tous les jours.
        let e = [
            entry(Some(0), Some(8), None, None, None),
            entry(Some(0), Some(20), None, None, None),
        ];
        assert_eq!(
            runs(&e, 3),
            ["2026-01-07 20:00", "2026-01-08 08:00", "2026-01-08 20:00"]
        );
    }

    #[test]
    fn minute_seule_toutes_les_heures() {
        let e = [entry(Some(45), None, None, None, None)];
        assert_eq!(runs(&e, 2), ["2026-01-07 12:45", "2026-01-07 13:45"]);
    }

    #[test]
    fn trentiere_fevrier_introuvable_sous_366_jours() {
        let e = [entry(Some(0), Some(0), Some(30), None, Some(2))];
        assert!(runs(&e, 1).is_empty());
    }

    #[test]
    fn round_trip_plist() {
        let entries = vec![
            entry(Some(0), Some(9), None, Some(1), None),
            entry(Some(30), Some(18), Some(1), None, Some(6)),
        ];
        let v = entries_to_plist(&entries);
        assert_eq!(entries_from_plist(&v).expect("array valide"), entries);
        // Entrée unique → dict, pas array (forme canonique launchd).
        let single = entries_to_plist(&entries[..1]);
        assert!(matches!(single, plist::Value::Dictionary(_)));
    }

    #[test]
    fn summary_lisible() {
        assert_eq!(
            summary(&[entry(Some(0), Some(4), None, None, None)]),
            "Minute=0 Hour=4"
        );
        assert_eq!(
            summary(&[
                entry(Some(0), Some(8), None, None, None),
                entry(Some(0), Some(20), None, None, None)
            ]),
            "StartCalendarInterval ×2"
        );
    }
}
