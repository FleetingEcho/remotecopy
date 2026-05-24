import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import HistoryList from "./components/HistoryList";
import ConnectForm from "./components/ConnectForm";
import type { SshStatus } from "./components/ConnectForm";
import Settings from "./components/Settings";
import Toast from "./components/Toast";
import type { ToastItem } from "./components/Toast";
import styles from "./App.module.css";
import type { Settings as SettingsType } from "./types";
import { LanguageProvider } from "./i18n/LanguageContext";
import type { Lang } from "./i18n/translations";
import { t as tt } from "./i18n/translations";

function applyTheme(theme: string) {
  const dark =
    theme === "dark" ||
    (theme === "auto" && window.matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.setAttribute("data-theme", dark ? "dark" : "light");
}

function fmtSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

type Tab = "connect" | "history" | "settings";

let toastIdCounter = 0;

function tabIcon(t: Tab): string {
  switch (t) {
    case "connect":  return "📡";
    case "history":  return "📋";
    case "settings": return "⚙️";
  }
}

function statusDot(s: SshStatus): string {
  switch (s.status) {
    case "disconnected": return "○";
    case "connecting":
    case "detecting":    return "◌";
    case "polling":      return "●";
    case "failed":       return "✕";
  }
}

function statusColor(s: SshStatus): string {
  switch (s.status) {
    case "disconnected": return "var(--pill-off-fg)";
    case "connecting":
    case "detecting":    return "var(--pill-busy-fg)";
    case "polling":      return "var(--pill-on-fg)";
    case "failed":       return "var(--pill-err-fg)";
  }
}

export default function App() {
  const [tab, setTab] = useState<Tab>("connect");
  const [sshStatus, setSshStatus] = useState<SshStatus>({ status: "disconnected" });
  const [lang, setLang] = useState<Lang>("en");
  const [langReady, setLangReady] = useState(false);
  const [localIps, setLocalIps] = useState<string[]>([]);
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const [historyVersion, setHistoryVersion] = useState(0);

  const dismissToast = useCallback((id: number) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  // Load settings + IPs on mount
  useEffect(() => {
    invoke<SettingsType>("get_settings")
      .then((s) => {
        applyTheme(s.theme ?? "auto");
        setLang((s.language as Lang) ?? "en");
        setLangReady(true);
      })
      .catch(() => { applyTheme("auto"); setLangReady(true); });

    invoke<string[]>("get_local_ips").then(setLocalIps).catch(() => {});

    const onThemeChange = (e: Event) =>
      applyTheme((e as CustomEvent<string>).detail);
    window.addEventListener("rc:theme", onThemeChange);

    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const onMqChange = () =>
      invoke<SettingsType>("get_settings")
        .then((s) => { if ((s.theme ?? "auto") === "auto") applyTheme("auto"); })
        .catch(() => {});
    mq.addEventListener("change", onMqChange);

    return () => {
      window.removeEventListener("rc:theme", onThemeChange);
      mq.removeEventListener("change", onMqChange);
    };
  }, []);

  // Listen to backend events
  useEffect(() => {
    const unlisten: (() => void)[] = [];
    (async () => {
      unlisten.push(
        await listen<SshStatus>("ssh-status", (e) => setSshStatus(e.payload))
      );
      unlisten.push(
        await listen<{ kind: string; preview: string; size: number }>(
          "clipboard-received",
          (e) => {
            setHistoryVersion((v) => v + 1);
            const id = ++toastIdCounter;
            const message =
              e.payload.kind === "image"
                ? `${tt(lang, "toast.imageReceived", { size: fmtSize(e.payload.size) })}`
                : `${tt(lang, "toast.textReceived")}${e.payload.preview ? ": " + e.payload.preview.slice(0, 40) : ""}`;
            setToasts((prev) => [...prev.slice(-4), { id, kind: e.payload.kind as "text" | "image", message }]);
          }
        )
      );
      invoke<SshStatus>("ssh_status").then(setSshStatus).catch(() => {});
    })();
    return () => unlisten.forEach((fn) => fn());
  }, [lang]);

  if (!langReady) return null;

  return (
    <LanguageProvider lang={lang}>
      <div className={styles.app}>
        {/* ── Brand header ── */}
        <header className={styles.header}>
          <div className={styles.brand}>
            <span className={styles.brandIcon}>🔗</span>
            <span className={styles.brandName}>{tt(lang, "app.brand")}</span>
          </div>
          <nav className={styles.nav}>
            {(["connect", "history", "settings"] as Tab[]).map((t) => (
              <button
                key={t}
                className={`${styles.tab} ${tab === t ? styles.activeTab : ""}`}
                onClick={() => setTab(t)}
              >
                <span className={styles.tabIcon}>{tabIcon(t)}</span>
                <span className={styles.tabLabel}>
                  {t === "connect" ? tt(lang, "app.tab.connect") :
                   t === "history" ? tt(lang, "app.tab.history") :
                   tt(lang, "app.tab.settings")}
                </span>
                {t === "connect" && (
                  <span className={styles.statusDot} style={{ color: statusColor(sshStatus) }}>
                    {statusDot(sshStatus)}
                  </span>
                )}
              </button>
            ))}
          </nav>
        </header>

        {/* ── Tab content (centered, max 800px) ── */}
        <section className={styles.content}>
          <div className={styles.tabPanel} style={{ display: tab === "connect" ? "flex" : "none" }}>
            <ConnectForm localIps={localIps} sshStatus={sshStatus} />
          </div>
          <div className={styles.tabPanel} style={{ display: tab === "history" ? "flex" : "none" }}>
            <HistoryList version={historyVersion} />
          </div>
          <div className={styles.tabPanel} style={{ display: tab === "settings" ? "flex" : "none" }}>
            <Settings onLanguageChange={(l) => setLang(l)} />
          </div>
        </section>

        <Toast toasts={toasts} onDismiss={dismissToast} />
      </div>
    </LanguageProvider>
  );
}
