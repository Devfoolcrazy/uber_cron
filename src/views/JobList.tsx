import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { Job } from "../lib/bindings";
import {
  formatRelative,
  formatRun,
  humanizeCalendar,
  humanizeCron,
  humanizeInterval,
  isReboot,
} from "../lib/schedule";

type Props = {
  jobs: Job[];
  loading: boolean;
  onCreate: () => void;
  onEdit: (job: Job) => void;
  onToggle: (job: Job) => void;
  onDelete: (job: Job) => void;
  onRun: (job: Job) => void;
  onLogs: (job: Job) => void;
};

function schedulePhrase(
  job: Job,
  t: (key: string) => string,
  language: string,
): string {
  switch (job.schedule.type) {
    case "cronExpr":
      if (isReboot(job.schedule.value)) return t("job.atReboot");
      return humanizeCron(job.schedule.value, language) ?? t("job.invalidExpr");
    case "calendarIntervals":
      return humanizeCalendar(job.schedule.value, language);
    case "interval":
      return humanizeInterval(job.schedule.value, language);
    case "none":
      return t("job.noSchedule");
  }
}

function ScheduleCell({ job }: { job: Job }) {
  const { t, i18n } = useTranslation();
  return (
    <div className="schedule-cell">
      <span className="schedule-phrase">{schedulePhrase(job, t, i18n.language)}</span>
      <code className="schedule-expr">{job.scheduleRaw}</code>
    </div>
  );
}

function JobRow({
  job,
  onEdit,
  onToggle,
  onDelete,
  onRun,
  onLogs,
}: { job: Job } & Omit<Props, "jobs" | "loading" | "onCreate">) {
  const { t, i18n } = useTranslation();
  const [confirming, setConfirming] = useState(false);
  const next = job.nextRuns[0];
  const isLaunchd = job.backend === "launchd";
  const hasLogs =
    isLaunchd && (job.launchdSpec?.stdoutPath != null || job.launchdSpec?.stderrPath != null);
  const editable = !isLaunchd || job.launchdSpec !== null;
  const runnable = !isLaunchd || job.status !== "notloaded";

  return (
    <li className={job.enabled ? "job-row" : "job-row disabled"}>
      <span
        className={
          job.status === "running"
            ? "status-dot running"
            : job.enabled
              ? "status-dot on"
              : "status-dot"
        }
        title={
          job.status === "running"
            ? t("job.running")
            : job.enabled
              ? t("job.enabled")
              : t("job.disabled")
        }
        aria-label={job.enabled ? t("job.enabled") : t("job.disabled")}
      />

      <div className="job-main">
        <div className="job-head">
          <span className="job-label">
            {job.label}
            {isLaunchd && (
              <span className={job.managed ? "tag managed" : "tag external"}>
                {t(job.managed ? "job.managedTag" : "job.externalTag")}
              </span>
            )}
            {job.lastExitCode !== null && (
              <span
                className={job.lastExitCode === 0 ? "tag exit-ok" : "tag exit-fail"}
                title={t("job.lastExit", { code: job.lastExitCode })}
              >
                {job.lastExitCode === 0 ? "✓ 0" : `✕ ${job.lastExitCode}`}
              </span>
            )}
          </span>
          {next ? (
            <span className="next-run" title={formatRun(next, i18n.language)}>
              {t("job.nextRun", { when: formatRelative(next, i18n.language) })}
            </span>
          ) : (
            <span className="next-run muted">
              {job.schedule.type === "cronExpr" && isReboot(job.schedule.value)
                ? t("job.atReboot")
                : job.schedule.type === "interval" || job.schedule.type === "none"
                  ? ""
                  : t("job.noNextRun")}
            </span>
          )}
        </div>
        <ScheduleCell job={job} />
        <code className="job-command" title={job.command}>
          {job.command}
        </code>
      </div>

      <div className="job-actions">
        {confirming ? (
          <span className="confirm-inline">
            {isLaunchd && !job.managed
              ? t("job.deleteExternalConfirm")
              : t("job.deleteConfirm")}
            <button className="btn small danger" onClick={() => onDelete(job)}>
              {t("common.yes")}
            </button>
            <button className="btn small" onClick={() => setConfirming(false)}>
              {t("common.no")}
            </button>
          </span>
        ) : (
          <>
            <button
              className="icon-btn"
              onClick={() => onRun(job)}
              disabled={!runnable}
              title={runnable ? t("actions.run") : t("job.runNeedsEnabled")}
              aria-label={t("actions.run")}
            >
              <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
                <path d="M4 2.5v11l9-5.5-9-5.5z" />
              </svg>
            </button>
            {hasLogs && (
              <button
                className="icon-btn"
                onClick={() => onLogs(job)}
                title={t("actions.logs")}
                aria-label={t("actions.logs")}
              >
                <svg width="13" height="13" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                  <path
                    d="M4 2h6l3 3v9H4V2zM10 2v3h3M6 8h5M6 11h5"
                    stroke="currentColor"
                    strokeWidth="1.3"
                    strokeLinejoin="round"
                  />
                </svg>
              </button>
            )}
            <button
              className="icon-btn"
              onClick={() => onEdit(job)}
              disabled={!editable}
              title={editable ? t("actions.edit") : t("job.notEditable")}
              aria-label={t("actions.edit")}
            >
              <svg width="13" height="13" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                <path
                  d="M11.5 2.5l2 2L5 13l-2.5.5L3 11l8.5-8.5z"
                  stroke="currentColor"
                  strokeWidth="1.4"
                  strokeLinejoin="round"
                />
              </svg>
            </button>
            <button
              className="icon-btn"
              onClick={() => setConfirming(true)}
              title={t("actions.delete")}
              aria-label={t("actions.delete")}
            >
              <svg width="13" height="13" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                <path
                  d="M2.5 4h11M6.5 4V2.5h3V4M4 4l.7 9.5h6.6L12 4M6.5 7v4M9.5 7v4"
                  stroke="currentColor"
                  strokeWidth="1.3"
                  strokeLinecap="round"
                />
              </svg>
            </button>
            <label className="switch" title={job.enabled ? t("actions.disable") : t("actions.enable")}>
              <input
                type="checkbox"
                checked={job.enabled}
                onChange={() => onToggle(job)}
                aria-label={job.enabled ? t("actions.disable") : t("actions.enable")}
              />
              <span className="slider" />
            </label>
          </>
        )}
      </div>
    </li>
  );
}

type Filter = "all" | "managed" | "external";

export default function JobList(props: Props) {
  const { t } = useTranslation();
  const { jobs, loading, onCreate } = props;
  const [filter, setFilter] = useState<Filter>("all");

  if (loading) {
    return <div className="placeholder">{t("common.loading")}</div>;
  }

  if (jobs.length === 0) {
    return (
      <div className="empty-state">
        <code className="empty-glyph">* * * * *</code>
        <h2>{t("list.emptyTitle")}</h2>
        <p>{t("list.emptyBody")}</p>
        <button className="btn primary" onClick={onCreate}>
          {t("list.emptyCta")}
        </button>
      </div>
    );
  }

  const isLaunchd = jobs.some((job) => job.backend === "launchd");
  const filtered = jobs.filter(
    (job) =>
      filter === "all" ||
      (filter === "managed" ? job.managed : !job.managed),
  );

  return (
    <>
      <div className="list-head">
        <p className="list-count">{t("list.count", { count: filtered.length })}</p>
        {isLaunchd && (
          <div className="preset-row">
            {(["all", "managed", "external"] as const).map((key) => (
              <button
                key={key}
                type="button"
                className={filter === key ? "chip active" : "chip"}
                onClick={() => setFilter(key)}
              >
                {t(`list.filter.${key}`)}
              </button>
            ))}
          </div>
        )}
      </div>
      <ul className="job-list">
        {filtered.map((job) => (
          <JobRow key={job.id} job={job} {...props} />
        ))}
      </ul>
    </>
  );
}
