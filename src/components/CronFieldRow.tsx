import { useTranslation } from "react-i18next";

export type CronFieldKey = "minute" | "hour" | "day" | "month" | "weekday";

type Mode = "every" | "step" | "exact" | "weekdays" | "weekend" | "custom";

const CONFIG: Record<CronFieldKey, { min: number; max: number; steps: number[] }> = {
  minute: { min: 0, max: 59, steps: [2, 5, 10, 15, 20, 30] },
  hour: { min: 0, max: 23, steps: [2, 3, 4, 6, 8, 12] },
  day: { min: 1, max: 31, steps: [2, 3, 5, 7, 10, 15] },
  month: { min: 1, max: 12, steps: [2, 3, 4, 6] },
  weekday: { min: 0, max: 6, steps: [] },
};

/** Noms localisés : mois 1-12, jours cron 0-6 (0 = dimanche ; 7 normalisé à 0). */
function valueLabel(key: CronFieldKey, value: number, language: string): string {
  if (key === "month") {
    return new Intl.DateTimeFormat(language, { month: "long" }).format(new Date(2023, value - 1, 1));
  }
  if (key === "weekday") {
    // Le 1er janvier 2023 était un dimanche (cron 0).
    return new Intl.DateTimeFormat(language, { weekday: "long" }).format(new Date(2023, 0, 1 + value));
  }
  return String(value).padStart(2, "0");
}

function derive(part: string, key: CronFieldKey): { mode: Mode; step: number; exact: number } {
  const cfg = CONFIG[key];
  const base = { step: cfg.steps[0] ?? 2, exact: cfg.min };
  if (part === "*") return { mode: "every", ...base };

  const stepMatch = /^\*\/(\d+)$/.exec(part);
  if (stepMatch && cfg.steps.length > 0) {
    return { mode: "step", step: Number(stepMatch[1]), exact: cfg.min };
  }

  if (key === "weekday") {
    if (part === "1-5") return { mode: "weekdays", ...base };
    if (part === "0,6" || part === "6,0") return { mode: "weekend", ...base };
  }

  if (/^\d+$/.test(part)) {
    const n = key === "weekday" && part === "7" ? 0 : Number(part);
    if (n >= cfg.min && n <= cfg.max) return { mode: "exact", step: base.step, exact: n };
  }

  return { mode: "custom", ...base };
}

type Props = {
  fieldKey: CronFieldKey;
  part: string;
  /** L'utilisateur a explicitement choisi « personnalisé » : on n'auto-redérive pas. */
  customSticky: boolean;
  onChange: (part: string, customSticky: boolean) => void;
};

export default function CronFieldRow({ fieldKey, part, customSticky, onChange }: Props) {
  const { t, i18n } = useTranslation();
  const cfg = CONFIG[fieldKey];
  const derived = derive(part, fieldKey);
  const mode: Mode = customSticky ? "custom" : derived.mode;

  const stepOptions = cfg.steps.includes(derived.step)
    ? cfg.steps
    : [...cfg.steps, derived.step].sort((a, b) => a - b);

  const exactValues = Array.from({ length: cfg.max - cfg.min + 1 }, (_, i) => cfg.min + i);

  const selectMode = (next: Mode) => {
    switch (next) {
      case "every":
        return onChange("*", false);
      case "step":
        return onChange(`*/${derived.step}`, false);
      case "exact":
        return onChange(String(derived.exact), false);
      case "weekdays":
        return onChange("1-5", false);
      case "weekend":
        return onChange("0,6", false);
      case "custom":
        return onChange(part, true);
    }
  };

  return (
    <div className="cron-row">
      <span className="cron-row-label">{t(`editor.fields.${fieldKey}`)}</span>

      <select
        value={mode}
        onChange={(e) => selectMode(e.target.value as Mode)}
        aria-label={`${t(`editor.fields.${fieldKey}`)} — mode`}
      >
        <option value="every">{t("editor.mode.every")}</option>
        {cfg.steps.length > 0 && <option value="step">{t("editor.mode.step")}</option>}
        <option value="exact">{t("editor.mode.exact")}</option>
        {fieldKey === "weekday" && (
          <>
            <option value="weekdays">{t("editor.mode.weekdays")}</option>
            <option value="weekend">{t("editor.mode.weekend")}</option>
          </>
        )}
        <option value="custom">{t("editor.mode.custom")}</option>
      </select>

      {mode === "step" && (
        <select
          value={derived.step}
          onChange={(e) => onChange(`*/${e.target.value}`, false)}
          aria-label={`${t(`editor.fields.${fieldKey}`)} — N`}
        >
          {stepOptions.map((n) => (
            <option key={n} value={n}>
              {n}
            </option>
          ))}
        </select>
      )}

      {mode === "exact" && (
        <select
          value={derived.exact}
          onChange={(e) => onChange(e.target.value, false)}
          aria-label={`${t(`editor.fields.${fieldKey}`)} — ${t("editor.mode.exact")}`}
        >
          {exactValues.map((n) => (
            <option key={n} value={n}>
              {valueLabel(fieldKey, n, i18n.language)}
            </option>
          ))}
        </select>
      )}

      {mode === "custom" && (
        <input
          className="mono"
          type="text"
          value={part}
          onChange={(e) => onChange(e.target.value.trim() === "" ? "*" : e.target.value.trim(), true)}
          spellCheck={false}
          aria-label={`${t(`editor.fields.${fieldKey}`)} — ${t("editor.mode.custom")}`}
        />
      )}

      {(mode === "every" || mode === "weekdays" || mode === "weekend") && (
        <code className="cron-row-part">{part}</code>
      )}
    </div>
  );
}
