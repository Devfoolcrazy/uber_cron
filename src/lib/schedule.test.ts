import { describe, expect, it } from "vitest";
import { formatRelative, formatRun, humanizeCron, isReboot } from "./schedule";

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
