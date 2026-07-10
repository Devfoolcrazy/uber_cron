import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  commands,
  type ApiError,
  type BackendKind,
  type Job,
  type RunResult,
} from "./lib/bindings";
import JobList from "./views/JobList";
import JobEditorCron from "./views/JobEditorCron";
import JobEditorLaunchd from "./views/JobEditorLaunchd";
import Diagnostics from "./views/Diagnostics";
import RunResultSheet from "./components/RunResultSheet";
import LogViewer from "./components/LogViewer";
import "./App.css";

type View =
  | { kind: "list" }
  | { kind: "editor"; job: Job | null }
  | { kind: "diagnostics" };

type RunState = { job: Job; result: RunResult | null };

const BACKEND_KEY = "ubercron.backend";

export function apiErrorMessage(
  error: ApiError,
  t: (key: string, opts?: Record<string, unknown>) => string,
): string {
  const key = `errors.${error.code}`;
  const translated = t(key);
  return translated === key ? t("errors.unknown") : translated;
}

function App() {
  const { t, i18n } = useTranslation();
  const [backend, setBackend] = useState<BackendKind>(() =>
    localStorage.getItem(BACKEND_KEY) === "launchd" ? "launchd" : "cron",
  );
  const [view, setView] = useState<View>({ kind: "list" });
  const [jobs, setJobs] = useState<Job[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<ApiError | null>(null);
  const [run, setRun] = useState<RunState | null>(null);
  const [logsJob, setLogsJob] = useState<Job | null>(null);

  const loadJobs = useCallback(async () => {
    setLoading(true);
    const result = await commands.listJobs(backend);
    if (result.status === "ok") {
      setJobs(result.data);
      setError(null);
    } else {
      setJobs([]);
      setError(result.error);
    }
    setLoading(false);
  }, [backend]);

  useEffect(() => {
    void loadJobs();
  }, [loadJobs]);

  const selectBackend = (kind: BackendKind) => {
    localStorage.setItem(BACKEND_KEY, kind);
    setBackend(kind);
    setView({ kind: "list" });
  };

  // Contrat §3.2 : toute mutation invalide tous les JobId → re-fetch complet.
  const afterMutation = useCallback(
    async (result: { status: "ok" } | { status: "error"; error: ApiError }) => {
      if (result.status === "error") setError(result.error);
      else setError(null);
      await loadJobs();
    },
    [loadJobs],
  );

  const handleToggle = async (job: Job) => {
    await afterMutation(await commands.setJobEnabled(backend, job.id, !job.enabled));
  };

  const handleDelete = async (job: Job) => {
    await afterMutation(await commands.deleteJob(backend, job.id));
  };

  const handleRun = async (job: Job) => {
    setRun({ job, result: null });
    const result = await commands.runJob(backend, job.id);
    if (result.status === "ok") {
      setRun((current) =>
        current && current.job.id === job.id ? { job, result: result.data } : current,
      );
    } else {
      setRun(null);
      setError(result.error);
    }
  };

  const toggleLanguage = () => {
    void i18n.changeLanguage(i18n.language.startsWith("fr") ? "en" : "fr");
  };

  return (
    <div className="app">
      <header className="toolbar">
        <h1 className="app-title">{t("app.title")}</h1>

        <div className="segmented" role="tablist" aria-label="Backend">
          {(["cron", "launchd"] as const).map((kind) => (
            <button
              key={kind}
              role="tab"
              aria-selected={backend === kind}
              className={backend === kind ? "segment active" : "segment"}
              onClick={() => selectBackend(kind)}
            >
              {t(`backend.${kind}`)}
            </button>
          ))}
        </div>

        <div className="toolbar-spacer" />

        <button className="icon-btn" onClick={toggleLanguage} title="FR / EN">
          {i18n.language.startsWith("fr") ? "FR" : "EN"}
        </button>
        <button
          className="icon-btn"
          onClick={() => void loadJobs()}
          title={t("header.reload")}
          aria-label={t("header.reload")}
        >
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
            <path
              d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9M13.5 1.5v3h-3"
              stroke="currentColor"
              strokeWidth="1.5"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        </button>
        <button
          className={view.kind === "diagnostics" ? "icon-btn active" : "icon-btn"}
          onClick={() =>
            setView(view.kind === "diagnostics" ? { kind: "list" } : { kind: "diagnostics" })
          }
          title={t("header.diagnostics")}
          aria-label={t("header.diagnostics")}
        >
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
            <path
              d="M1.5 8h3l2-5 3 10 2-5h3"
              stroke="currentColor"
              strokeWidth="1.5"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        </button>
        <button className="btn primary" onClick={() => setView({ kind: "editor", job: null })}>
          {t("header.newJob")}
        </button>
      </header>

      <main className="content">
        {error && view.kind === "list" && (
          <div className="banner error" role="alert">
            <span>{apiErrorMessage(error, t)}</span>
            <button className="btn small" onClick={() => void loadJobs()}>
              {t("common.reload")}
            </button>
          </div>
        )}

        {view.kind === "list" && (
          <JobList
            jobs={jobs}
            loading={loading}
            onCreate={() => setView({ kind: "editor", job: null })}
            onEdit={(job) => setView({ kind: "editor", job })}
            onToggle={handleToggle}
            onDelete={handleDelete}
            onRun={handleRun}
            onLogs={setLogsJob}
          />
        )}

        {view.kind === "editor" &&
          (backend === "cron" ? (
            <JobEditorCron
              backend={backend}
              job={view.job}
              onSaved={() => {
                setView({ kind: "list" });
                void loadJobs();
              }}
              onCancel={() => setView({ kind: "list" })}
            />
          ) : (
            <JobEditorLaunchd
              job={view.job}
              onSaved={() => {
                setView({ kind: "list" });
                void loadJobs();
              }}
              onCancel={() => setView({ kind: "list" })}
            />
          ))}

        {view.kind === "diagnostics" && (
          <Diagnostics backend={backend} onClose={() => setView({ kind: "list" })} />
        )}
      </main>

      {run && (
        <RunResultSheet
          job={run.job}
          result={run.result}
          onClose={() => setRun(null)}
          onLogs={(job) => {
            setRun(null);
            setLogsJob(job);
          }}
        />
      )}

      {logsJob && <LogViewer job={logsJob} onClose={() => setLogsJob(null)} />}
    </div>
  );
}

export default App;
