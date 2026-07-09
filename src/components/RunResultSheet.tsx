import { useTranslation } from "react-i18next";
import type { Job, RunResult } from "../lib/bindings";

type Props = {
  job: Job;
  /** null pendant l'exécution. */
  result: RunResult | null;
  onClose: () => void;
};

export default function RunResultSheet({ job, result, onClose }: Props) {
  const { t } = useTranslation();
  const completed = result?.type === "completed" ? result.value : null;

  return (
    <div className="sheet-backdrop" onClick={onClose}>
      <section
        className="sheet"
        role="dialog"
        aria-modal="true"
        aria-label={job.label}
        onClick={(e) => e.stopPropagation()}
      >
        <header className="sheet-head">
          <div className="sheet-title">
            <span className="job-label">{job.label}</span>
            <code className="job-command">{job.command}</code>
          </div>
          {result === null ? (
            <span className="badge running">
              <span className="spinner" aria-hidden="true" /> {t("run.running")}
            </span>
          ) : completed ? (
            <span className={completed.exitCode === 0 ? "badge ok" : "badge fail"}>
              {completed.exitCode === 0
                ? t("run.exitOk")
                : t("run.exitFail", { code: completed.exitCode })}
            </span>
          ) : null}
          <button className="icon-btn" onClick={onClose} aria-label={t("common.close")}>
            ✕
          </button>
        </header>

        {completed && (
          <div className="sheet-body">
            <h4 className="panel-subtitle">{t("run.stdout")}</h4>
            <pre className="output">{completed.stdoutTail || t("run.empty")}</pre>
            <h4 className="panel-subtitle">{t("run.stderr")}</h4>
            <pre className="output stderr">{completed.stderrTail || t("run.empty")}</pre>
            <p className="hint">{t("run.envNote")}</p>
          </div>
        )}
      </section>
    </div>
  );
}
