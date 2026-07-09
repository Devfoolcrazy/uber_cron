import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type BackendKind, type Diagnostic } from "../lib/bindings";

type Props = {
  backend: BackendKind;
  onClose: () => void;
};

const SEVERITY_GLYPH: Record<Diagnostic["severity"], string> = {
  ok: "✓",
  warning: "⚠",
  error: "✕",
};

export default function Diagnostics({ backend, onClose }: Props) {
  const { t } = useTranslation();
  const [checks, setChecks] = useState<Diagnostic[] | null>(null);

  useEffect(() => {
    void commands.runDiagnostics(backend).then((result) => {
      setChecks(result.status === "ok" ? result.data : []);
    });
  }, [backend]);

  return (
    <div className="diagnostics">
      <div className="view-head">
        <h2 className="view-title">
          {t("diag.title")} — {t(`backend.${backend}`)}
        </h2>
        <button className="btn" onClick={onClose}>
          {t("common.close")}
        </button>
      </div>

      {checks === null ? (
        <div className="placeholder">{t("common.loading")}</div>
      ) : (
        <ul className="diag-list">
          {checks.map((check) => (
            <li key={check.code} className={`diag-item ${check.severity}`}>
              <span className="diag-glyph" aria-hidden="true">
                {SEVERITY_GLYPH[check.severity]}
              </span>
              <div>
                <p className="diag-title">{t(`diag.${check.code}.title`)}</p>
                <p className="diag-body">{t(`diag.${check.code}.body`)}</p>
                {check.detail && <code className="diag-detail">{check.detail}</code>}
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
