import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import type { Settings as SettingsType } from "../types";
import styles from "./Settings.module.css";
import { useLanguage } from "../i18n/LanguageContext";
import Toggle from "./Toggle";
import SegmentedControl from "./SegmentedControl";

interface Props {
  onLanguageChange?: (lang: "en" | "zh") => void;
}

export default function Settings({ onLanguageChange }: Props) {
  const { t } = useLanguage();
  const [settings, setSettings] = useState<SettingsType | null>(null);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState("");
  const [dbPathInput, setDbPathInput] = useState("");

  useEffect(() => {
    invoke<SettingsType>("get_settings").then((s) => {
      setSettings(s);
      setDbPathInput(s.db_path);
    });
  }, []);

  useEffect(() => {
    if (settings?.theme) {
      window.dispatchEvent(new CustomEvent("rc:theme", { detail: settings.theme }));
    }
  }, [settings?.theme]);

  function update<K extends keyof SettingsType>(key: K, value: SettingsType[K]) {
    setSettings((prev) => prev ? { ...prev, [key]: value } : prev);
  }

  function handleLanguageChange(lang: "en" | "zh") {
    if (!settings) return;
    const updated = { ...settings, language: lang };
    setSettings(updated);
    invoke("save_settings", { settings: updated }).catch(() => {});
    onLanguageChange?.(lang);
  }

  async function applyDbPath() {
    if (!settings || !dbPathInput.trim()) return;
    const updated = { ...settings, db_path: dbPathInput.trim() };
    setSettings(updated);
    try {
      await invoke("save_settings", { settings: updated });
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      setError(String(e));
    }
  }

  async function pickDbPath() {
    const selected = await saveDialog({
      filters: [{ name: "SQLite Database", extensions: ["db"] }],
      defaultPath: dbPathInput,
    });
    if (typeof selected === "string") setDbPathInput(selected);
  }

  async function save() {
    if (!settings) return;
    setError("");
    try {
      await invoke("save_settings", { settings });
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      setError(String(e));
    }
  }

  if (!settings) {
    return <div className={styles.loading}>{t("settings.loading")}</div>;
  }

  const themeOptions = [
    { label: t("settings.themeDark"), value: "dark" },
    { label: t("settings.themeLight"), value: "light" },
    { label: t("settings.themeAuto"), value: "auto" },
  ];
  const langOptions = [
    { label: t("settings.langEn"), value: "en" },
    { label: t("settings.langZh"), value: "zh" },
  ];

  return (
    <div className={styles.pane}>
      <div className={styles.scroll}>

        {/* ── Appearance ───────────────────────────── */}
        <section className={styles.section}>
          <h3 className={styles.sectionTitle}>{t("settings.appearance")}</h3>
          <div className={styles.card}>
            <div className={styles.row}>
              <span className={styles.rowLabel}>{t("settings.theme")}</span>
              <SegmentedControl
                options={themeOptions}
                value={settings.theme ?? "auto"}
                onChange={(v) => update("theme", v)}
              />
            </div>
            <div className={styles.divider} />
            <div className={styles.row}>
              <span className={styles.rowLabel}>{t("settings.language")}</span>
              <SegmentedControl
                options={langOptions}
                value={settings.language ?? "en"}
                onChange={(v) => handleLanguageChange(v as "en" | "zh")}
              />
            </div>
          </div>
        </section>

        {/* ── Allowed Content ──────────────────────── */}
        <section className={styles.section}>
          <h3 className={styles.sectionTitle}>{t("settings.allowedContent")}</h3>
          <div className={styles.card}>
            <div className={styles.row}>
              <span className={styles.rowLabel}>{t("settings.contentText")}</span>
              <Toggle
                checked={settings.allow_text}
                onChange={(v) => update("allow_text", v)}
              />
            </div>
            <div className={styles.divider} />
            <div className={styles.row}>
              <span className={styles.rowLabel}>{t("settings.contentImage")}</span>
              <Toggle
                checked={settings.allow_image}
                onChange={(v) => update("allow_image", v)}
              />
            </div>
          </div>
        </section>

        {/* ── History ──────────────────────────────── */}
        <section className={styles.section}>
          <h3 className={styles.sectionTitle}>{t("settings.history")}</h3>
          <div className={styles.card}>
            <div className={styles.row}>
              <span className={styles.rowLabel}>{t("settings.maxPayload")}</span>
              <div className={styles.sliderRow}>
                <input
                  type="range" min={1} max={500}
                  value={settings.max_payload_mb}
                  onChange={(e) => update("max_payload_mb", Number(e.target.value))}
                />
                <span className={styles.sliderVal}>{settings.max_payload_mb} MB</span>
              </div>
            </div>
            <div className={styles.divider} />
            <div className={styles.row}>
              <span className={styles.rowLabel}>{t("settings.retention")}</span>
              <div className={styles.sliderRow}>
                <input
                  type="range" min={1} max={365}
                  value={settings.history_retention_days}
                  onChange={(e) => update("history_retention_days", Number(e.target.value))}
                />
                <span className={styles.sliderVal}>{settings.history_retention_days} {t("settings.retention").split("(")[1]?.replace(")", "") || "days"}</span>
              </div>
            </div>
          </div>
        </section>

        {/* ── Database ─────────────────────────────── */}
        <section className={styles.section}>
          <h3 className={styles.sectionTitle}>{t("settings.database")}</h3>
          <div className={styles.card}>
            <div className={styles.rowCol}>
              <span className={styles.rowLabel}>{t("settings.dbPath")}</span>
              <div className={styles.pathRow}>
                <input
                  type="text"
                  value={dbPathInput}
                  onChange={(e) => setDbPathInput(e.target.value)}
                  className={styles.pathInput}
                />
                <button className={styles.smallBtn} onClick={pickDbPath}>
                  {t("settings.browse")}
                </button>
                <button
                  className={styles.smallBtn}
                  onClick={applyDbPath}
                  disabled={dbPathInput === settings.db_path}
                >
                  {t("settings.dbApply")}
                </button>
              </div>
              <p className={styles.hint}>{t("settings.dbHint")}</p>
            </div>
          </div>
        </section>

        {/* ── System ───────────────────────────────── */}
        <section className={styles.section}>
          <h3 className={styles.sectionTitle}>{t("settings.system")}</h3>
          <div className={styles.card}>
            <div className={styles.row}>
              <span className={styles.rowLabel}>{t("settings.startWithWindows")}</span>
              <Toggle
                checked={settings.start_with_windows}
                onChange={(v) => update("start_with_windows", v)}
              />
            </div>
          </div>
        </section>

      </div>

      <div className={styles.footer}>
        {error && <span className={styles.error}>{error}</span>}
        <button
          className={`${styles.saveBtn} ${saved ? styles.ok : ""}`}
          onClick={save}
        >
          {saved ? t("settings.saved") : t("settings.saveSettings")}
        </button>
      </div>
    </div>
  );
}
