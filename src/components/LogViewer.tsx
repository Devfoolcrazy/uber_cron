import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type Job } from "../lib/bindings";

type Props = {
  job: Job;
  onClose: () => void;
};

function Pane({ title, path }: { title: string; path: string | null }) {
  const { t } = useTranslation();
  const [content, setContent] = useState<string | null>(null);

  const load = useCallback(() => {
    if (!path) return;
    void commands.readLogTail(path).then((result) => {
      setContent(result.status === "ok" ? result.data : null);
    });
  }, [path]);

  useEffect(load, [load]);

  if (!path) return null;
  return (
    <>
      <h4 className="panel-subtitle">
        {title} <code className="log-path">{path}</code>
      </h4>
      <pre className="output">{content?.trim() ? content : t("logs.empty")}</pre>
    </>
  );
}

export default function LogViewer({ job, onClose }: Props) {
  const { t } = useTranslation();
  const [refreshKey, setRefreshKey] = useState(0);
  const stdout = job.launchdSpec?.stdoutPath ?? null;
  const stderr = job.launchdSpec?.stderrPath ?? null;

  return (
    <div className="sheet-backdrop" onClick={onClose}>
      <section
        className="sheet"
        role="dialog"
        aria-modal="true"
        aria-label={t("logs.title", { label: job.label })}
        onClick={(e) => e.stopPropagation()}
      >
        <header className="sheet-head">
          <div className="sheet-title">
            <span className="job-label">{t("logs.title", { label: job.label })}</span>
          </div>
          <button className="btn small" onClick={() => setRefreshKey(refreshKey + 1)}>
            {t("logs.refresh")}
          </button>
          <button className="icon-btn" onClick={onClose} aria-label={t("common.close")}>
            ✕
          </button>
        </header>
        <div className="sheet-body" key={refreshKey}>
          <Pane title={t("logs.stdout")} path={stdout} />
          <Pane title={t("logs.stderr")} path={stderr} />
        </div>
      </section>
    </div>
  );
}
