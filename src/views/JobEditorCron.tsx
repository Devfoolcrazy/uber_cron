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

const FIELD_KEYS = ["minute", "hour", "day", "month", "weekday"] as const;

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
  const [command, setCommand] = useState(job?.command ?? "");
  const [preview, setPreview] = useState<SchedulePreview | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);

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

  const commandNeedsAbsolutePath =
    command.trim() !== "" && !/^[/~]/.test(command.trim());

  const canSave =
    !saving && command.trim() !== "" && (preview === null || preview.valid);

  const save = async () => {
    setSaving(true);
    setError(null);
    const spec = {
      type: "cron" as const,
      value: {
        schedule: expression.trim(),
        command: command.trim(),
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
              <div className="cron-fields">
                {FIELD_KEYS.map((key, index) => (
                  <label key={key} className="cron-field">
                    <input
                      className="mono"
                      type="text"
                      value={fields[index]}
                      onChange={(e) => setField(index, e.target.value)}
                      spellCheck={false}
                      aria-label={t(`editor.fields.${key}`)}
                    />
                    <span>{t(`editor.fields.${key}`)}</span>
                  </label>
                ))}
              </div>
            )}
          </fieldset>

          <label className="field">
            <span className="field-label">{t("editor.command")}</span>
            <input
              className="mono"
              type="text"
              value={command}
              onChange={(e) => setCommand(e.target.value)}
              placeholder={t("editor.commandPlaceholder")}
              spellCheck={false}
            />
          </label>
          {commandNeedsAbsolutePath && (
            <p className="hint warning">{t("editor.pathWarning")}</p>
          )}

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
