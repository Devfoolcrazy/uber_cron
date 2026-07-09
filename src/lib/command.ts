// Décomposition structurée d'une commande cron (décision §12 + UI commande) :
// [cd DOSSIER && ] PROGRAMME [ >> JOURNAL 2>&1 ]
// La ligne composée reste l'unique chose écrite dans la crontab ; la
// décomposition ne sert qu'à l'éditeur. Un motif non reconnu reste
// intégralement dans `program` (aucune perte possible).

export type CommandParts = {
  program: string;
  workdir: string;
  log: string;
};

export function composeCommand({ program, workdir, log }: CommandParts): string {
  let cmd = program.trim();
  if (workdir.trim() !== "") cmd = `cd ${workdir.trim()} && ${cmd}`;
  if (log.trim() !== "") cmd = `${cmd} >> ${log.trim()} 2>&1`;
  return cmd;
}

export function decomposeCommand(command: string): CommandParts {
  let program = command.trim();
  let workdir = "";
  let log = "";

  // Journal : uniquement un `>> fichier 2>&1` terminal, sans espaces dans le
  // chemin — sinon on ne décompose pas (le motif reste dans program).
  const logMatch = /^(.*?)\s*>>\s*(\S+)\s*2>&1$/.exec(program);
  if (logMatch) {
    program = logMatch[1].trim();
    log = logMatch[2];
  }

  // Dossier de travail : `cd chemin && …` en tête, chemin sans espaces.
  const cdMatch = /^cd\s+(\S+)\s*&&\s*(.+)$/s.exec(program);
  if (cdMatch) {
    workdir = cdMatch[1];
    program = cdMatch[2].trim();
  }

  return { program, workdir, log };
}

/** Chemin de journal proposé : ~/Library/Logs existe toujours sur macOS,
 * et `$HOME` est développé par le sh de cron (pas de mkdir nécessaire). */
export function suggestLogPath(name: string, program: string): string {
  const firstToken = program.trim().split(/\s+/)[0] ?? "";
  const base = name.trim() !== "" ? name : (firstToken.split("/").pop() ?? "");
  const slug =
    base
      .normalize("NFD")
      .replace(/[\u0300-\u036f]/g, "")
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "") || "tache";
  return `$HOME/Library/Logs/ubercron-${slug}.log`;
}
