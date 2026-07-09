import { describe, expect, it } from "vitest";
import { composeCommand, decomposeCommand, suggestLogPath } from "./command";

const METEO =
  "cd /Users/moi/POC/scrape_meteo_agricole && /Users/moi/POC/scrape_meteo_agricole/venv/bin/python /Users/moi/POC/scrape_meteo_agricole/wrapper.py >> /Users/moi/POC/scrape_meteo_agricole/cron.log 2>&1";

describe("decomposeCommand", () => {
  it("décompose le motif complet cd && prog >> log 2>&1", () => {
    const parts = decomposeCommand(METEO);
    expect(parts.workdir).toBe("/Users/moi/POC/scrape_meteo_agricole");
    expect(parts.program).toBe(
      "/Users/moi/POC/scrape_meteo_agricole/venv/bin/python /Users/moi/POC/scrape_meteo_agricole/wrapper.py",
    );
    expect(parts.log).toBe("/Users/moi/POC/scrape_meteo_agricole/cron.log");
  });

  it("round-trip : compose(decompose(x)) == x", () => {
    expect(composeCommand(decomposeCommand(METEO))).toBe(METEO);
  });

  it("commande simple : tout dans program", () => {
    const parts = decomposeCommand("/usr/bin/uptime");
    expect(parts).toEqual({ program: "/usr/bin/uptime", workdir: "", log: "" });
  });

  it("journal seul, sans dossier de travail", () => {
    const parts = decomposeCommand("/usr/local/bin/backup --full >> $HOME/backup.log 2>&1");
    expect(parts.program).toBe("/usr/local/bin/backup --full");
    expect(parts.log).toBe("$HOME/backup.log");
    expect(parts.workdir).toBe("");
  });

  it("ne décompose PAS un >> non terminal ou un cd à chemin avec espaces", () => {
    const chained = "/bin/cmd >> a.log 2>&1 && /bin/autre";
    expect(decomposeCommand(chained).program).toBe(chained);
    const spacedCd = 'cd "/Users/moi/Mon Dossier" && /bin/cmd';
    expect(decomposeCommand(spacedCd).workdir).toBe("");
    expect(decomposeCommand(spacedCd).program).toBe(spacedCd);
  });

  it("un programme contenant && après décomposition du cd initial round-trippe", () => {
    const cmd = "cd /tmp && /bin/a && /bin/b";
    const parts = decomposeCommand(cmd);
    expect(parts.workdir).toBe("/tmp");
    expect(parts.program).toBe("/bin/a && /bin/b");
    expect(composeCommand(parts)).toBe(cmd);
  });
});

describe("composeCommand", () => {
  it("n'ajoute que ce qui est renseigné", () => {
    expect(composeCommand({ program: "/bin/x", workdir: "", log: "" })).toBe("/bin/x");
    expect(composeCommand({ program: "/bin/x", workdir: "/tmp", log: "" })).toBe(
      "cd /tmp && /bin/x",
    );
    expect(composeCommand({ program: "/bin/x", workdir: "", log: "/tmp/x.log" })).toBe(
      "/bin/x >> /tmp/x.log 2>&1",
    );
  });
});

describe("suggestLogPath", () => {
  it("utilise le nom du job en priorité, sinon le binaire", () => {
    expect(suggestLogPath("Scrap météo", "python x.py")).toBe(
      "$HOME/Library/Logs/ubercron-scrap-meteo.log",
    );
    expect(suggestLogPath("", "/usr/local/bin/backup --full")).toBe(
      "$HOME/Library/Logs/ubercron-backup.log",
    );
    expect(suggestLogPath("", "")).toBe("$HOME/Library/Logs/ubercron-tache.log");
  });
});
