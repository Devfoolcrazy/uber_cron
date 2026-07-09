import { useState } from "react";
import { useTranslation } from "react-i18next";
import { humanizeCron } from "../lib/schedule";

/** Exemples pédagogiques : la phrase est générée par cronstrue (localisée),
 * cliquer remplit l'expression et l'aperçu live fait le reste. */
const EXAMPLES = [
  "*/5 * * * *",
  "0 * * * *",
  "30 8 * * *",
  "0 9 * * 1-5",
  "*/15 9-18 * * 1-5",
  "0 0 1 * *",
  "0 8 1,15 * *",
  "30 22 * * 0",
  "0 10 * * 6,0",
  "15 7 1 1,4,7,10 *",
];

const FIELD_RANGES: Array<{ key: string; range: string }> = [
  { key: "minute", range: "0-59" },
  { key: "hour", range: "0-23" },
  { key: "day", range: "1-31" },
  { key: "month", range: "1-12 · jan-dec" },
  { key: "weekday", range: "0-7 · mon-sun" },
];

const OPERATORS: Array<{ op: string; key: string }> = [
  { op: "*", key: "star" },
  { op: ",", key: "comma" },
  { op: "-", key: "dash" },
  { op: "/", key: "slash" },
];

export default function CronHelp({ onUse }: { onUse: (expr: string) => void }) {
  const { t, i18n } = useTranslation();
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
          <div className="help-columns">
            <section>
              <h4 className="panel-subtitle">{t("editor.help.syntaxTitle")}</h4>
              <table className="help-table">
                <tbody>
                  {FIELD_RANGES.map(({ key, range }) => (
                    <tr key={key}>
                      <th scope="row">{t(`editor.fields.${key}`)}</th>
                      <td>
                        <code>{range}</code>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
              <p className="hint">{t("editor.help.weekdayNote")}</p>
            </section>

            <section>
              <h4 className="panel-subtitle">{t("editor.help.operatorsTitle")}</h4>
              <table className="help-table">
                <tbody>
                  {OPERATORS.map(({ op, key }) => (
                    <tr key={key}>
                      <th scope="row">
                        <code>{op}</code>
                      </th>
                      <td>{t(`editor.help.op.${key}`)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </section>
          </div>

          <h4 className="panel-subtitle">{t("editor.help.examplesTitle")}</h4>
          <ul className="example-list">
            {EXAMPLES.map((expr) => (
              <li key={expr}>
                <button type="button" className="example-btn" onClick={() => onUse(expr)}>
                  <code>{expr}</code>
                  <span>{humanizeCron(expr, i18n.language)}</span>
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
