import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  commands,
  type ApiError,
  type BackendKind,
  type Job,
  type SchedulePreview,
} from "../lib/bindings";
import { formatRun, humanizeCron, isReboot } from "../lib/schedule";
import { apiErrorMessage } from "../App";
import CommandHelp from "../components/CommandHelp";
import CronFieldRow, { type CronFieldKey } from "../components/CronFieldRow";
import CronHelp from "../components/CronHelp";
import {
  composeCommand,
  decomposeCommand,
  suggestLogPath,
  type CommandParts,
} from "../lib/command";

type Props = {
  backend: BackendKind;
  job: Job | null;
  onSaved: () => void;
  onCancel: () => void;
};

const PRESETS = [
  { key: "everyMinute", expr: "* * * * *" },
  { key: "hourly", expr: "0 * * * *" },
  { key: "daily", expr: "0 9 * * *" },
  { key: "weekly", expr: "0 9 * * 1" },
  { key: "monthly", expr: "0 9 1 * *" },
] as const;

const FIELD_KEYS: readonly CronFieldKey[] = ["minute", "hour", "day", "month", "weekday"];
const NO_CUSTOM = [false, false, false, false, false];

/** Découpe une expression 5 champs ; null si ce n'en est pas une (@raccourci…). */
function splitFields(expr: string): string[] | null {
  const parts = expr.trim().split(/\s+/);
  return parts.length === 5 ? parts : null;
}

export default function JobEditorCron({ backend, job, onSaved, onCancel }: Props) {
  const { t, i18n } = useTranslation();
  const initialExpr = job?.scheduleRaw ?? "0 9 * * 1";
  const [expression, setExpression] = useState(initialExpr);
  const [freeMode, setFreeMode] = useState(splitFields(initialExpr) === null);
  const [name, setName] = useState(job?.name ?? "");
  const [cmdParts, setCmdParts] = useState<CommandParts>(() =>
    decomposeCommand(job?.command ?? ""),
  );
  const [preview, setPreview] = useState<SchedulePreview | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  // Champs passés explicitement en mode « personnalisé » (pas d'auto-redérivation).
  const [customFields, setCustomFields] = useState<boolean[]>(NO_CUSTOM);

  const fields = splitFields(expression);

  // Aperçu live (§8.1) : debounce court sur la frappe.
  useEffect(() => {
    const handle = setTimeout(() => {
      void commands
        .previewSchedule({ type: "cronExpr", value: expression })
        .then(setPreview);
    }, 200);
    return () => clearTimeout(handle);
  }, [expression]);

  const phrase = useMemo(() => {
    if (isReboot(expression)) return t("job.atReboot");
    return humanizeCron(expression, i18n.language);
  }, [expression, i18n.language, t]);

  const setField = (index: number, value: string) => {
    const current = splitFields(expression) ?? ["*", "*", "*", "*", "*"];
    current[index] = value.trim() === "" ? "*" : value.trim();
    setExpression(current.join(" "));
  };

  const composed = composeCommand(cmdParts);
  const setPart = (part: keyof CommandParts) => (value: string) =>
    setCmdParts((current) => ({ ...current, [part]: value }));

  // Conseils contextuels sur la commande (§11 PATH + journal).
  const firstToken = cmdParts.program.trim().split(/\s+/)[0] ?? "";
  const hintBareProgram =
    firstToken !== "" && !firstToken.includes("/") && !/^[~$]/.test(firstToken);
  const hintRelativeProgram =
    firstToken.includes("/") &&
    !/^[/~$]/.test(firstToken) &&
    cmdParts.workdir.trim() === "";
  const hintWorkdirRelative =
    cmdParts.workdir.trim() !== "" && !/^[/~$]/.test(cmdParts.workdir.trim());
  const hintNoLog = cmdParts.program.trim() !== "" && cmdParts.log.trim() === "";

  const canSave =
    !saving && cmdParts.program.trim() !== "" && (preview === null || preview.valid);

  const save = async () => {
    setSaving(true);
    setError(null);
    const spec = {
      type: "cron" as const,
      value: {
        schedule: expression.trim(),
        command: composed,
        name: name.trim() === "" ? null : name.trim(),
      },
    };
    const result = job
      ? await commands.updateJob(backend, job.id, spec)
      : await commands.createJob(backend, spec);
    setSaving(false);
    if (result.status === "ok") onSaved();
    else setError(result.error);
  };

  return (
    <div className="editor">
      <h2 className="view-title">{job ? t("editor.titleEdit") : t("editor.titleNew")}</h2>

      {error && (
        <div className="banner error" role="alert">
          <span>{apiErrorMessage(error, t)}</span>
        </div>
      )}

      <div className="editor-grid">
        <form
          className="editor-form"
          onSubmit={(e) => {
            e.preventDefault();
            if (canSave) void save();
          }}
        >
          <label className="field">
            <span className="field-label">
              {t("editor.name")} <em>({t("editor.nameOptional")})</em>
            </span>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={t("editor.namePlaceholder")}
            />
          </label>

          <fieldset className="field">
            <legend className="field-label">{t("editor.schedule")}</legend>
            <div className="preset-row">
              {PRESETS.map((preset) => (
                <button
                  key={preset.key}
                  type="button"
                  className={
                    expression.trim() === preset.expr ? "chip active" : "chip"
                  }
                  onClick={() => {
                    setExpression(preset.expr);
                    setFreeMode(false);
                    setCustomFields(NO_CUSTOM);
                  }}
                >
                  {t(`editor.presets.${preset.key}`)}
                </button>
              ))}
              <button
                type="button"
                className={freeMode ? "chip active" : "chip"}
                onClick={() => setFreeMode(!freeMode)}
              >
                {freeMode ? t("editor.builderMode") : t("editor.freeMode")}
              </button>
            </div>

            {freeMode || fields === null ? (
              <label className="field">
                <span className="field-label">{t("editor.expression")}</span>
                <input
                  className="mono"
                  type="text"
                  value={expression}
                  onChange={(e) => setExpression(e.target.value)}
                  spellCheck={false}
                />
              </label>
            ) : (
              <div className="cron-rows">
                {FIELD_KEYS.map((key, index) => (
                  <CronFieldRow
                    key={key}
                    fieldKey={key}
                    part={fields[index]}
                    customSticky={customFields[index]}
                    onChange={(part, sticky) => {
                      setField(index, part);
                      setCustomFields((current) =>
                        current.map((c, i) => (i === index ? sticky : c)),
                      );
                    }}
                  />
                ))}
              </div>
            )}

            <CronHelp
              onUse={(expr) => {
                setExpression(expr);
                setFreeMode(false);
                setCustomFields(NO_CUSTOM);
              }}
            />
          </fieldset>

          <label className="field">
            <span className="field-label">{t("editor.command")}</span>
            <input
              className="mono"
              type="text"
              value={cmdParts.program}
              onChange={(e) => setPart("program")(e.target.value)}
              placeholder={t("editor.commandPlaceholder")}
              spellCheck={false}
            />
          </label>
          {hintBareProgram && (
            <p className="hint warning">
              {t("editor.hints.bareProgram", { name: firstToken })}
            </p>
          )}
          {hintRelativeProgram && <p className="hint warning">{t("editor.pathWarning")}</p>}

          <div className="cmd-extras">
            <label className="field">
              <span className="field-label">
                {t("editor.workdir")} <em>({t("editor.nameOptional")})</em>
              </span>
              <input
                className="mono"
                type="text"
                value={cmdParts.workdir}
                onChange={(e) => setPart("workdir")(e.target.value)}
                placeholder="/Users/moi/mon-projet"
                spellCheck={false}
              />
            </label>
            <label className="field">
              <span className="field-label">
                {t("editor.log")} <em>({t("editor.nameOptional")})</em>
              </span>
              <input
                className="mono"
                type="text"
                value={cmdParts.log}
                onChange={(e) => setPart("log")(e.target.value)}
                placeholder="$HOME/Library/Logs/ubercron-tache.log"
                spellCheck={false}
              />
            </label>
          </div>
          {hintWorkdirRelative && (
            <p className="hint warning">{t("editor.hints.workdirRelative")}</p>
          )}
          {hintNoLog && (
            <p className="hint">
              {t("editor.hints.noLog")}{" "}
              <button
                type="button"
                className="link-btn inline"
                onClick={() => setPart("log")(suggestLogPath(name, cmdParts.program))}
              >
                {t("editor.hints.addLog")}
              </button>
            </p>
          )}
          {(cmdParts.workdir.trim() !== "" || cmdParts.log.trim() !== "") &&
            cmdParts.program.trim() !== "" && (
              <p className="field">
                <span className="field-label">{t("editor.generated")}</span>
                <code className="generated-line">{composed}</code>
              </p>
            )}

          <CommandHelp onUse={setCmdParts} />

          <div className="editor-actions">
            <button type="button" className="btn" onClick={onCancel}>
              {t("common.cancel")}
            </button>
            <button type="submit" className="btn primary" disabled={!canSave}>
              {job ? t("editor.saveEdit") : t("editor.saveCreate")}
            </button>
          </div>
        </form>

        <aside className="preview-panel" aria-live="polite">
          <h3 className="panel-title">{t("editor.preview")}</h3>
          {preview && !preview.valid ? (
            <p className="preview-phrase invalid">{t("editor.previewInvalid")}</p>
          ) : (
            <>
              <p className="preview-phrase">{phrase ?? "…"}</p>
              <code className="schedule-expr">{expression.trim()}</code>
              {preview && preview.nextRuns.length > 0 && (
                <>
                  <h4 className="panel-subtitle">{t("editor.nextRunsTitle")}</h4>
                  <ol className="next-runs">
                    {preview.nextRuns.map((iso) => (
                      <li key={iso}>{formatRun(iso, i18n.language)}</li>
                    ))}
                  </ol>
                </>
              )}
            </>
          )}
        </aside>
      </div>
    </div>
  );
}
