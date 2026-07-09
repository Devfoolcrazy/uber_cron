// Humanisation des schedules côté frontend (décision §12.6) :
// cronstrue pour les expressions cron, Intl pour les dates.

import cronstrue from "cronstrue";
import "cronstrue/locales/fr";
import "cronstrue/locales/en";

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
