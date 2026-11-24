// Settings → General → Text and languages: the language of Magnetite's own texts.
import { send } from "../bridge";
import { language, LANGUAGES, languageName, t } from "../i18n";
import { Field } from "./fields";

/**
 * The system's language (`system`, what the app resolves it to) or one of
 * `LANGUAGES`, each named in itself. It counts from the next start: while
 * the choice differs from what the page shows, a button restarts Magnetite.
 */
export function LanguageField({ chosen, system, save }: { chosen: string | null; system: string; save: (language: string | null) => void }) {
  const next = chosen ?? system;
  return (
    <Field label={t("settings.language.label")} hint={t("settings.language.hint")}>
      <select className="field" value={chosen ?? ""} onChange={(e) => save(e.target.value || null)}>
        <option value="">{t("settings.language.system", { language: languageName(system) })}</option>
        {LANGUAGES.map((tag) => (
          <option key={tag} value={tag} lang={tag}>
            {languageName(tag)}
          </option>
        ))}
      </select>
      {next !== language && (
        <div className="mt-2 flex flex-wrap items-center gap-3">
          <p className="set-hint">{t("settings.language.after_restart", { language: languageName(next) })}</p>
          <button className="btn" onClick={() => void send({ cmd: "restart" })}>
            {t("settings.language.restart")}
          </button>
        </div>
      )}
    </Field>
  );
}
