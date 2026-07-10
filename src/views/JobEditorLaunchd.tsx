import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  commands,
  type ApiError,
  type CalendarEntry,
  type Job,
  type LaunchdJobSpec,
  type SchedulePreview,
} from "../lib/bindings";
import { formatRun, humanizeCalendar, humanizeInterval, monthName, weekdayName } from "../lib/schedule";
import { apiErrorMessage } from "../App";

type Props = {
  job: Job | null;
  onSaved: () => void;
  onCancel: () => void;
};

const EMPTY_ENTRY: CalendarEntry = {
  minute: 0,
  hour: 9,
  day: null,
  weekday: null,
  month: null,
};

type Unit = "seconds" | "minutes" | "hours";
const UNIT_FACTOR: Record<Unit, number> = { seconds: 1, minutes: 60, hours: 3600 };

function initialInterval(spec: LaunchdJobSpec | null): { value: number; unit: Unit } {
  if (spec?.schedule.type !== "interval") return { value: 1, unit: "hours" };
  const secs = spec.schedule.value;
  if (secs % 3600 === 0) return { value: secs / 3600, unit: "hours" };
  if (secs % 60 === 0) return { value: secs / 60, unit: "minutes" };
  return { value: secs, unit: "seconds" };
}

/** Select « — / valeur » d'un champ d'entrée calendar. */
function EntryField({
  label,
  value,
  values,
  display,
  onChange,
}: {
  label: string;
  value: number | null;
  values: number[];
  display: (n: number) => string;
  onChange: (v: number | null) => void;
}) {
  return (
    <label className="cron-field">
      <select
        value={value === null ? "" : String(value)}
        onChange={(e) => onChange(e.target.value === "" ? null : Number(e.target.value))}
        aria-label={label}
      >
        <option value="">—</option>
        {values.map((n) => (
          <option key={n} value={n}>
            {display(n)}
          </option>
        ))}
      </select>
      <span>{label}</span>
    </label>
  );
}

const range = (from: number, to: number) =>
  Array.from({ length: to - from + 1 }, (_, i) => from + i);

export default function JobEditorLaunchd({ job, onSaved, onCancel }: Props) {
  const { t, i18n } = useTranslation();
  const spec = job?.launchdSpec ?? null;

  const [label, setLabel] = useState(spec?.label ?? "com.ubercron.");
  const [cmdMode, setCmdMode] = useState<"shell" | "argv">(
    spec?.command.type === "argv" ? "argv" : "shell",
  );
  const [shellCmd, setShellCmd] = useState(
    spec?.command.type === "shellWrapper" ? spec.command.value : "",
  );
  const [argvText, setArgvText] = useState(
    spec?.command.type === "argv" ? spec.command.value.join("\n") : "",
  );
  const [schedType, setSchedType] = useState<"calendar" | "interval">(
    spec?.schedule.type === "interval" ? "interval" : "calendar",
  );
  const [entries, setEntries] = useState<CalendarEntry[]>(
    spec?.schedule.type === "calendarIntervals" ? spec.schedule.value : [EMPTY_ENTRY],
  );
  const [interval, setInterval] = useState(initialInterval(spec));
  const [runAtLoad, setRunAtLoad] = useState(spec?.runAtLoad ?? false);
  const [workdir, setWorkdir] = useState(spec?.workingDirectory ?? "");
  const [stdoutPath, setStdoutPath] = useState(spec?.stdoutPath ?? "");
  const [stderrPath, setStderrPath] = useState(spec?.stderrPath ?? "");
  const [home, setHome] = useState("");
  const [preview, setPreview] = useState<SchedulePreview | null>(null);
  const [saving, setSaving] = useState(false);
  const [confirmKill, setConfirmKill] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);

  useEffect(() => {
    void commands.homeDir().then(setHome);
  }, []);

  const intervalSecs = Math.max(0, Math.round(interval.value * UNIT_FACTOR[interval.unit]));

  useEffect(() => {
    if (schedType !== "calendar") {
      setPreview(null);
      return;
    }
    const handle = setTimeout(() => {
      void commands
        .previewSchedule({ type: "calendarIntervals", value: entries })
        .then(setPreview);
    }, 200);
    return () => clearTimeout(handle);
  }, [schedType, entries]);

  const phrase = useMemo(() => {
    if (schedType === "interval") return humanizeInterval(intervalSecs, i18n.language);
    return humanizeCalendar(entries, i18n.language);
  }, [schedType, entries, intervalSecs, i18n.language]);

  const labelValid = /^[A-Za-z0-9._-]+$/.test(label.trim()) && label.trim() !== "com.ubercron.";
  const argv = argvText
    .split("\n")
    .map((s) => s.trim())
    .filter((s) => s !== "");
  const commandValid = cmdMode === "shell" ? shellCmd.trim() !== "" : argv.length > 0;

  // Conseils anti-pièges, identiques à l'éditeur cron (§11 PATH).
  const firstToken =
    cmdMode === "shell" ? (shellCmd.trim().split(/\s+/)[0] ?? "") : (argv[0] ?? "");
  const hintBareProgram =
    firstToken !== "" && !firstToken.includes("/") && !firstToken.startsWith("~");
  const hintRelativeProgram =
    firstToken.includes("/") && !/^[/~]/.test(firstToken) && workdir.trim() === "";
  const hintWorkdirRelative = workdir.trim() !== "" && !/^[/~]/.test(workdir.trim());
  const scheduleValid =
    schedType === "calendar" ? entries.length > 0 : intervalSecs > 0;

  const canSave = !saving && labelValid && commandValid && scheduleValid;

  const setEntry = (index: number, entry: CalendarEntry) =>
    setEntries((current) => current.map((e, i) => (i === index ? entry : e)));

  const suggestLogs = () => {
    if (home === "") return;
    const base = `${home}/Library/Logs/${label.trim()}`;
    setStdoutPath(`${base}.out.log`);
    setStderrPath(`${base}.err.log`);
  };

  const save = async () => {
    // Garde-fou §6.4 : enregistrer tue le process en cours (bootout).
    if (job?.status === "running" && !confirmKill) {
      setConfirmKill(true);
      return;
    }
    setSaving(true);
    setError(null);
    const value: LaunchdJobSpec = {
      label: label.trim(),
      command:
        cmdMode === "shell"
          ? { type: "shellWrapper", value: shellCmd.trim() }
          : { type: "argv", value: argv },
      schedule:
        schedType === "calendar"
          ? { type: "calendarIntervals", value: entries }
          : { type: "interval", value: intervalSecs },
      runAtLoad,
      workingDirectory: workdir.trim() === "" ? null : workdir.trim(),
      stdoutPath: stdoutPath.trim() === "" ? null : stdoutPath.trim(),
      stderrPath: stderrPath.trim() === "" ? null : stderrPath.trim(),
    };
    const specPayload = { type: "launchd" as const, value };
    const result = job
      ? await commands.updateJob("launchd", job.id, specPayload)
      : await commands.createJob("launchd", specPayload);
    setSaving(false);
    if (result.status === "ok") onSaved();
    else setError(result.error);
  };

  return (
    <div className="editor">
      <h2 className="view-title">
        {job ? t("editor.launchd.titleEdit") : t("editor.launchd.titleNew")}
      </h2>

      {job && !job.managed && (
        <div className="banner warning-banner" role="alert">
          {t("editor.launchd.externalWarning")}
        </div>
      )}
      {confirmKill && (
        <div className="banner warning-banner" role="alert">
          {t("editor.launchd.killWarning")}
        </div>
      )}
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
            <span className="field-label">{t("editor.launchd.label")}</span>
            <input
              className="mono"
              type="text"
              value={label}
              onChange={(e) => setLabel(e.target.value)}
              disabled={job !== null}
              spellCheck={false}
            />
            {job === null && <span className="hint">{t("editor.launchd.labelHint")}</span>}
          </label>

          <fieldset className="field">
            <legend className="field-label">{t("editor.command")}</legend>
            <div className="preset-row">
              {(["shell", "argv"] as const).map((mode) => (
                <button
                  key={mode}
                  type="button"
                  className={cmdMode === mode ? "chip active" : "chip"}
                  onClick={() => setCmdMode(mode)}
                >
                  {t(`editor.launchd.commandMode.${mode}`)}
                </button>
              ))}
            </div>
            {cmdMode === "shell" ? (
              <input
                className="mono"
                type="text"
                value={shellCmd}
                onChange={(e) => setShellCmd(e.target.value)}
                placeholder={t("editor.commandPlaceholder")}
                spellCheck={false}
              />
            ) : (
              <>
                <textarea
                  className="mono argv-input"
                  value={argvText}
                  onChange={(e) => setArgvText(e.target.value)}
                  placeholder={t("editor.launchd.argvPlaceholder")}
                  spellCheck={false}
                  rows={4}
                />
                <span className="hint">{t("editor.launchd.argvHint")}</span>
              </>
            )}
            {hintBareProgram && (
              <p className="hint warning">
                {t("editor.hints.bareProgram", { name: firstToken })}
              </p>
            )}
            {hintRelativeProgram && <p className="hint warning">{t("editor.pathWarning")}</p>}

            <label className="field workdir-field">
              <span className="field-label">
                {t("editor.workdir")} <em>({t("editor.nameOptional")})</em>
              </span>
              <input
                className="mono"
                type="text"
                value={workdir}
                onChange={(e) => setWorkdir(e.target.value)}
                placeholder="/Users/moi/mon-projet"
                spellCheck={false}
              />
            </label>
            {hintWorkdirRelative && (
              <p className="hint warning">{t("editor.hints.workdirRelative")}</p>
            )}
          </fieldset>

          <fieldset className="field">
            <legend className="field-label">{t("editor.schedule")}</legend>
            <div className="preset-row">
              {(["calendar", "interval"] as const).map((type) => (
                <button
                  key={type}
                  type="button"
                  className={schedType === type ? "chip active" : "chip"}
                  onClick={() => setSchedType(type)}
                >
                  {t(`editor.launchd.scheduleType.${type}`)}
                </button>
              ))}
            </div>

            {schedType === "calendar" ? (
              <div className="entry-list">
                {entries.map((entry, index) => (
                  <div key={index} className="entry-row">
                    <EntryField
                      label={t("editor.fields.minute")}
                      value={entry.minute}
                      values={range(0, 59)}
                      display={(n) => String(n).padStart(2, "0")}
                      onChange={(minute) => setEntry(index, { ...entry, minute })}
                    />
                    <EntryField
                      label={t("editor.fields.hour")}
                      value={entry.hour}
                      values={range(0, 23)}
                      display={(n) => String(n).padStart(2, "0")}
                      onChange={(hour) => setEntry(index, { ...entry, hour })}
                    />
                    <EntryField
                      label={t("editor.fields.day")}
                      value={entry.day}
                      values={range(1, 31)}
                      display={String}
                      onChange={(day) => setEntry(index, { ...entry, day })}
                    />
                    <EntryField
                      label={t("editor.fields.weekday")}
                      value={entry.weekday}
                      values={range(0, 6)}
                      display={(n) => weekdayName(n, i18n.language)}
                      onChange={(weekday) => setEntry(index, { ...entry, weekday })}
                    />
                    <EntryField
                      label={t("editor.fields.month")}
                      value={entry.month}
                      values={range(1, 12)}
                      display={(n) => monthName(n, i18n.language)}
                      onChange={(month) => setEntry(index, { ...entry, month })}
                    />
                    {entries.length > 1 && (
                      <button
                        type="button"
                        className="icon-btn"
                        title={t("editor.launchd.removeEntry")}
                        aria-label={t("editor.launchd.removeEntry")}
                        onClick={() =>
                          setEntries((current) => current.filter((_, i) => i !== index))
                        }
                      >
                        ✕
                      </button>
                    )}
                  </div>
                ))}
                <button
                  type="button"
                  className="link-btn"
                  onClick={() => setEntries((current) => [...current, { ...EMPTY_ENTRY }])}
                >
                  + {t("editor.launchd.addEntry")}
                </button>
              </div>
            ) : (
              <div className="interval-row">
                <span>{t("editor.launchd.interval")}</span>
                <input
                  className="mono interval-value"
                  type="number"
                  min={1}
                  value={interval.value}
                  onChange={(e) =>
                    setInterval({ ...interval, value: Number(e.target.value) })
                  }
                />
                <select
                  value={interval.unit}
                  onChange={(e) => setInterval({ ...interval, unit: e.target.value as Unit })}
                >
                  {(["seconds", "minutes", "hours"] as const).map((unit) => (
                    <option key={unit} value={unit}>
                      {t(`editor.launchd.unit.${unit}`)}
                    </option>
                  ))}
                </select>
              </div>
            )}
          </fieldset>

          <label className="checkbox-row">
            <input
              type="checkbox"
              checked={runAtLoad}
              onChange={(e) => setRunAtLoad(e.target.checked)}
            />
            <span>{t("editor.launchd.runAtLoad")}</span>
          </label>

          <div className="cmd-extras">
            <label className="field">
              <span className="field-label">{t("editor.launchd.stdout")}</span>
              <input
                className="mono"
                type="text"
                value={stdoutPath}
                onChange={(e) => setStdoutPath(e.target.value)}
                spellCheck={false}
              />
            </label>
            <label className="field">
              <span className="field-label">{t("editor.launchd.stderr")}</span>
              <input
                className="mono"
                type="text"
                value={stderrPath}
                onChange={(e) => setStderrPath(e.target.value)}
                spellCheck={false}
              />
            </label>
          </div>
          {stdoutPath.trim() === "" && stderrPath.trim() === "" && (
            <p className="hint">
              {t("editor.hints.noLog")}{" "}
              <button type="button" className="link-btn inline" onClick={suggestLogs}>
                {t("editor.launchd.suggestLogs")}
              </button>
            </p>
          )}

          <div className="editor-actions">
            <button type="button" className="btn" onClick={onCancel}>
              {t("common.cancel")}
            </button>
            <button
              type="submit"
              className={confirmKill ? "btn danger" : "btn primary"}
              disabled={!canSave}
            >
              {confirmKill
                ? t("editor.launchd.confirmKill")
                : job
                  ? t("editor.saveEdit")
                  : t("editor.launchd.saveCreate")}
            </button>
          </div>
        </form>

        <aside className="preview-panel" aria-live="polite">
          <h3 className="panel-title">{t("editor.preview")}</h3>
          {preview && !preview.valid && schedType === "calendar" ? (
            <p className="preview-phrase invalid">{t("editor.previewInvalid")}</p>
          ) : (
            <>
              <p className="preview-phrase">{phrase}</p>
              {schedType === "interval" && (
                <p className="hint">{t("editor.launchd.intervalNote")}</p>
              )}
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
