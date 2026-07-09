import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { Job } from "../lib/bindings";
import { formatRelative, formatRun, humanizeCron, isReboot } from "../lib/schedule";

type Props = {
  jobs: Job[];
  loading: boolean;
  onCreate: () => void;
  onEdit: (job: Job) => void;
  onToggle: (job: Job) => void;
  onDelete: (job: Job) => void;
  onRun: (job: Job) => void;
};

function ScheduleCell({ job }: { job: Job }) {
  const { t, i18n } = useTranslation();
  const phrase = isReboot(job.scheduleRaw)
    ? t("job.atReboot")
    : (humanizeCron(job.scheduleRaw, i18n.language) ?? t("job.invalidExpr"));
  return (
    <div className="schedule-cell">
      <span className="schedule-phrase">{phrase}</span>
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
}: { job: Job } & Omit<Props, "jobs" | "loading" | "onCreate">) {
  const { t, i18n } = useTranslation();
  const [confirming, setConfirming] = useState(false);
  const next = job.nextRuns[0];

  return (
    <li className={job.enabled ? "job-row" : "job-row disabled"}>
      <span
        className={job.enabled ? "status-dot on" : "status-dot"}
        title={job.enabled ? t("job.enabled") : t("job.disabled")}
        aria-label={job.enabled ? t("job.enabled") : t("job.disabled")}
      />

      <div className="job-main">
        <div className="job-head">
          <span className="job-label">{job.label}</span>
          {next ? (
            <span className="next-run" title={formatRun(next, i18n.language)}>
              {t("job.nextRun", { when: formatRelative(next, i18n.language) })}
            </span>
          ) : (
            <span className="next-run muted">
              {isReboot(job.scheduleRaw) ? t("job.atReboot") : t("job.noNextRun")}
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
            {t("job.deleteConfirm")}
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
              title={t("actions.run")}
              aria-label={t("actions.run")}
            >
              <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
                <path d="M4 2.5v11l9-5.5-9-5.5z" />
              </svg>
            </button>
            <button
              className="icon-btn"
              onClick={() => onEdit(job)}
              title={t("actions.edit")}
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

export default function JobList(props: Props) {
  const { t } = useTranslation();
  const { jobs, loading, onCreate } = props;

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

  return (
    <>
      <p className="list-count">{t("list.count", { count: jobs.length })}</p>
      <ul className="job-list">
        {jobs.map((job) => (
          <JobRow key={job.id} job={job} {...props} />
        ))}
      </ul>
    </>
  );
}
