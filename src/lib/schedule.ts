// Humanisation des schedules côté frontend (décision §12.6) :
// cronstrue pour les expressions cron, Intl pour les dates.

import cronstrue from "cronstrue";
import "cronstrue/locales/fr";
import "cronstrue/locales/en";
import type { CalendarEntry } from "./bindings";

const SHORTCUTS: Record<string, string> = {
  "@yearly": "0 0 1 1 *",
  "@annually": "0 0 1 1 *",
  "@monthly": "0 0 1 * *",
  "@weekly": "0 0 * * 0",
  "@daily": "0 0 * * *",
  "@midnight": "0 0 * * *",
  "@hourly": "0 * * * *",
};

export function isReboot(expr: string): boolean {
  return expr.trim().toLowerCase() === "@reboot";
}

/** Phrase humaine d'une expression cron, ou null si non humanisable
 * (@reboot a sa propre clé i18n ; expression invalide → null). */
export function humanizeCron(expr: string, language: string): string | null {
  const trimmed = expr.trim();
  if (trimmed === "" || isReboot(trimmed)) return null;
  const mapped = SHORTCUTS[trimmed.toLowerCase()] ?? trimmed;
  try {
    return cronstrue.toString(mapped, {
      locale: language.startsWith("fr") ? "fr" : "en",
      use24HourTimeFormat: true,
      verbose: false,
    });
  } catch {
    return null;
  }
}

/** Noms localisés (0 = dimanche pour les jours, 1-12 pour les mois). */
export function weekdayName(value: number, language: string): string {
  // Le 1er janvier 2023 était un dimanche.
  return new Intl.DateTimeFormat(language, { weekday: "long" }).format(
    new Date(2023, 0, 1 + value),
  );
}

export function monthName(value: number, language: string): string {
  return new Intl.DateTimeFormat(language, { month: "long" }).format(
    new Date(2023, value - 1, 1),
  );
}

const two = (n: number) => String(n).padStart(2, "0");
const isFr = (language: string) => language.startsWith("fr");

/** Phrase humaine d'une entrée StartCalendarInterval. Champs absents =
 * toutes les valeurs (sémantique launchd). FR/EN, comme cronstrue. */
function humanizeEntry(e: CalendarEntry, language: string): string {
  const fr = isFr(language);

  const dateParts: string[] = [];
  if (e.weekday !== null) {
    dateParts.push(fr ? `le ${weekdayName(e.weekday, language)}` : `on ${weekdayName(e.weekday, language)}`);
  }
  if (e.day !== null) {
    dateParts.push(fr ? `le ${e.day}` : `on day ${e.day}`);
  }
  if (e.month !== null) {
    dateParts.push(fr ? `en ${monthName(e.month, language)}` : `in ${monthName(e.month, language)}`);
  }
  const datePhrase = dateParts.join(" ");

  if (e.hour !== null && e.minute !== null) {
    const time = `${two(e.hour)}:${two(e.minute)}`;
    const day = datePhrase || (fr ? "chaque jour" : "every day");
    return fr ? `${day} à ${time}` : `${day} at ${time}`;
  }
  if (e.hour === null && e.minute !== null) {
    const base = fr
      ? `à la minute ${e.minute} de chaque heure`
      : `at minute ${e.minute} of every hour`;
    return datePhrase ? `${datePhrase}, ${base}` : base;
  }
  if (e.hour !== null && e.minute === null) {
    const base = fr
      ? `chaque minute de ${two(e.hour)} h`
      : `every minute of hour ${two(e.hour)}`;
    return datePhrase ? `${datePhrase}, ${base}` : base;
  }
  const base = fr ? "chaque minute" : "every minute";
  return datePhrase ? `${datePhrase}, ${base}` : base;
}

/** Phrase humaine d'un StartCalendarInterval complet (§8 — équivalent
 * cronstrue pour launchd, généré côté frontend, décision §12.6). */
export function humanizeCalendar(entries: CalendarEntry[], language: string): string {
  return entries.map((e) => humanizeEntry(e, language)).join(" · ");
}

/** Phrase humaine d'un StartInterval en secondes. */
export function humanizeInterval(seconds: number, language: string): string {
  const fr = isFr(language);
  const unit = (n: number, frOne: string, frMany: string, enOne: string, enMany: string) => {
    if (n === 1) return fr ? frOne : enOne;
    return fr ? `${frMany.replace("{n}", String(n))}` : `${enMany.replace("{n}", String(n))}`;
  };
  if (seconds % 86400 === 0) {
    return unit(seconds / 86400, "chaque jour", "tous les {n} jours", "every day", "every {n} days");
  }
  if (seconds % 3600 === 0) {
    return unit(seconds / 3600, "toutes les heures", "toutes les {n} heures", "every hour", "every {n} hours");
  }
  if (seconds % 60 === 0) {
    return unit(seconds / 60, "chaque minute", "toutes les {n} minutes", "every minute", "every {n} minutes");
  }
  return fr ? `toutes les ${seconds} secondes` : `every ${seconds} seconds`;
}

/** "lun. 12 janv., 09:00" — date absolue courte d'une occurrence RFC3339. */
export function formatRun(iso: string, language: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return new Intl.DateTimeFormat(language, {
    weekday: "short",
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

/** "dans 2 heures" / "in 2 hours" — temps relatif jusqu'à l'occurrence. */
export function formatRelative(iso: string, language: string, now: Date = new Date()): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  const seconds = Math.round((date.getTime() - now.getTime()) / 1000);
  const rtf = new Intl.RelativeTimeFormat(language, { numeric: "auto" });
  const abs = Math.abs(seconds);
  if (abs < 60) return rtf.format(seconds, "second");
  if (abs < 3600) return rtf.format(Math.round(seconds / 60), "minute");
  if (abs < 86400) return rtf.format(Math.round(seconds / 3600), "hour");
  return rtf.format(Math.round(seconds / 86400), "day");
}
