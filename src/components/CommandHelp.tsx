import { useState } from "react";
import { useTranslation } from "react-i18next";
import { composeCommand, type CommandParts } from "../lib/command";

/** Exemples de commandes typiques : cliquer remplit les trois champs,
 * l'utilisateur n'a plus qu'à adapter les chemins. */
const EXAMPLES: Array<{ key: string; parts: CommandParts }> = [
  {
    key: "shell",
    parts: {
      program: "/Users/moi/scripts/sauvegarde.sh",
      workdir: "",
      log: "$HOME/Library/Logs/ubercron-sauvegarde.log",
    },
  },
  {
    key: "python",
    parts: {
      program: "venv/bin/python mon_script.py",
      workdir: "/Users/moi/mon-projet",
      log: "$HOME/Library/Logs/ubercron-mon-projet.log",
    },
  },
  {
    key: "node",
    parts: {
      program: "/opt/homebrew/bin/node index.js",
      workdir: "/Users/moi/mon-app",
      log: "$HOME/Library/Logs/ubercron-mon-app.log",
    },
  },
  {
    key: "rsync",
    parts: {
      program: "/usr/bin/rsync -a $HOME/Documents /Volumes/Sauvegarde/",
      workdir: "",
      log: "$HOME/Library/Logs/ubercron-rsync.log",
    },
  },
  {
    key: "cleanup",
    parts: {
      program: '/usr/bin/find $HOME/Downloads -name "*.dmg" -mtime +30 -delete',
      workdir: "",
      log: "",
    },
  },
  {
    key: "curl",
    parts: {
      program: "/usr/bin/curl -s https://exemple.com/ping",
      workdir: "",
      log: "",
    },
  },
];

const TIPS = ["absolute", "venv", "workdir", "log", "test"] as const;

export default function CommandHelp({ onUse }: { onUse: (parts: CommandParts) => void }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);

  return (
    <div className="cron-help-wrap">
      <button
        type="button"
        className="link-btn"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      >
        {open ? "▾" : "▸"} {t("editor.help.toggle")}
      </button>

      {open && (
        <div className="cron-help">
          <h4 className="panel-subtitle">{t("editor.cmdHelp.tipsTitle")}</h4>
          <ul className="tips-list">
            {TIPS.map((tip) => (
              <li key={tip}>{t(`editor.cmdHelp.tips.${tip}`)}</li>
            ))}
          </ul>

          <h4 className="panel-subtitle">{t("editor.help.examplesTitle")}</h4>
          <ul className="example-list">
            {EXAMPLES.map(({ key, parts }) => (
              <li key={key}>
                <button type="button" className="example-btn cmd" onClick={() => onUse(parts)}>
                  <span className="example-cmd-label">
                    {t(`editor.cmdHelp.examples.${key}.label`)}
                    <em> — {t(`editor.cmdHelp.examples.${key}.desc`)}</em>
                  </span>
                  <code>{composeCommand(parts)}</code>
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
