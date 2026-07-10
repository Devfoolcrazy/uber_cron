import { describe, expect, it } from "vitest";
import {
  formatRelative,
  formatRun,
  humanizeCalendar,
  humanizeCron,
  humanizeInterval,
  isReboot,
} from "./schedule";
import type { CalendarEntry } from "./bindings";

function entry(partial: Partial<CalendarEntry>): CalendarEntry {
  return { minute: null, hour: null, day: null, weekday: null, month: null, ...partial };
}

describe("humanizeCalendar", () => {
  it("jour + heure précise", () => {
    expect(humanizeCalendar([entry({ minute: 0, hour: 4 })], "fr")).toBe(
      "chaque jour à 04:00",
    );
    expect(humanizeCalendar([entry({ minute: 0, hour: 4 })], "en")).toBe(
      "every day at 04:00",
    );
  });

  it("jour de semaine nommé", () => {
    expect(humanizeCalendar([entry({ minute: 30, hour: 9, weekday: 1 })], "fr")).toBe(
      "le lundi à 09:30",
    );
    expect(humanizeCalendar([entry({ minute: 30, hour: 9, weekday: 0 })], "en")).toBe(
      "on Sunday at 09:30",
    );
  });

  it("jour du mois et mois nommé", () => {
    expect(humanizeCalendar([entry({ minute: 0, hour: 8, day: 1, month: 1 })], "fr")).toBe(
      "le 1 en janvier à 08:00",
    );
  });

  it("champs manquants = toutes les valeurs", () => {
    expect(humanizeCalendar([entry({ minute: 45 })], "fr")).toBe(
      "à la minute 45 de chaque heure",
    );
    expect(humanizeCalendar([entry({})], "fr")).toBe("chaque minute");
  });

  it("plusieurs entrées jointes", () => {
    const phrase = humanizeCalendar(
      [entry({ minute: 0, hour: 8 }), entry({ minute: 0, hour: 20 })],
      "fr",
    );
    expect(phrase).toBe("chaque jour à 08:00 · chaque jour à 20:00");
  });
});

describe("humanizeInterval", () => {
  it("unités lisibles", () => {
    expect(humanizeInterval(3600, "fr")).toBe("toutes les heures");
    expect(humanizeInterval(7200, "fr")).toBe("toutes les 2 heures");
    expect(humanizeInterval(900, "en")).toBe("every 15 minutes");
    expect(humanizeInterval(86400, "fr")).toBe("chaque jour");
    expect(humanizeInterval(90, "fr")).toBe("toutes les 90 secondes");
  });
});

describe("humanizeCron", () => {
  it("humanise une expression 5 champs en français", () => {
    const phrase = humanizeCron("0 9 * * 1", "fr");
    expect(phrase).toContain("09:00");
    expect(phrase?.toLowerCase()).toContain("lundi");
  });

  it("humanise en anglais en format 24h", () => {
    const phrase = humanizeCron("0 9 * * 1", "en");
    expect(phrase).toContain("09:00");
    expect(phrase?.toLowerCase()).toContain("monday");
  });

  it("traduit les @-raccourcis", () => {
    expect(humanizeCron("@daily", "fr")).toContain("00:00");
    expect(humanizeCron("@hourly", "en")).toBeTruthy();
    expect(humanizeCron("@midnight", "fr")).toContain("00:00");
  });

  it("@reboot n'est pas humanisable par cronstrue (clé i18n dédiée)", () => {
    expect(humanizeCron("@reboot", "fr")).toBeNull();
    expect(isReboot(" @REBOOT ")).toBe(true);
  });

  it("expression invalide → null", () => {
    expect(humanizeCron("99 99 * * *", "fr")).toBeNull();
    expect(humanizeCron("n'importe quoi", "fr")).toBeNull();
    expect(humanizeCron("", "fr")).toBeNull();
  });
});

describe("formatRun / formatRelative", () => {
  const iso = "2026-01-12T09:00:00+01:00";

  it("formate une date absolue lisible", () => {
    const fr = formatRun(iso, "fr");
    expect(fr).toContain("09:00");
    expect(fr.toLowerCase()).toContain("janv");
  });

  it("formate un temps relatif", () => {
    const now = new Date("2026-01-12T07:00:00+01:00");
    expect(formatRelative(iso, "fr", now)).toBe("dans 2 heures");
    expect(formatRelative(iso, "en", now)).toBe("in 2 hours");
  });

  it("ne casse pas sur une date invalide", () => {
    expect(formatRun("garbage", "fr")).toBe("garbage");
  });
});
