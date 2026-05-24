import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type { Settings } from "../types";
import styles from "./ConnectForm.module.css";
import { useLanguage } from "../i18n/LanguageContext";

export type SshStatus =
  | { status: "disconnected" }
  | { status: "connecting" }
  | { status: "detecting" }
  | { status: "polling"; host: string; since: string; method: string; supports_image: boolean }
  | { status: "failed"; reason: string };

interface BackendInfo {
  method: string;
  supports_image: boolean;
}

interface Props {
  localIps: string[];
  sshStatus: SshStatus;
}

export default function ConnectForm({ localIps, sshStatus: initialStatus }: Props) {
  const { t } = useLanguage();
  const [host, setHost] = useState("");
  const [port, setPort] = useState(22);
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [savePw, setSavePw] = useState(false);
  const [keyPath, setKeyPath] = useState("");
  const [keyValid, setKeyValid] = useState<boolean | null>(null);
  const [keyChecking, setKeyChecking] = useState(false);
  const [status, setStatus] = useState<SshStatus>(initialStatus);
  const [error, setError] = useState("");
  const [copiedIp, setCopiedIp] = useState<string | null>(null);
  const [backend, setBackend] = useState<BackendInfo | null>(null);
  const [lastReceived, setLastReceived] = useState<string | null>(null);

  useEffect(() => { setStatus(initialStatus); }, [initialStatus]);

  useEffect(() => {
    invoke<Settings>("get_settings").then((s) => {
      setHost(s.ssh.host);
      setPort(s.ssh.port);
      setUsername(s.ssh.username);
      setKeyPath(s.ssh.key_path);
      if (s.ssh.password) { setPassword(s.ssh.password); setSavePw(true); }
    });
  }, []);

  useEffect(() => {
    const u1 = listen<SshStatus>("ssh-status", (e) => setStatus(e.payload));
    const u2 = listen<BackendInfo>("clipboard-backend", (e) => setBackend(e.payload));
    const u3 = listen<{ created_at: string }>("clipboard-received", (e) => {
      setLastReceived(e.payload.created_at);
    });
    return () => {
      u1.then((fn) => fn());
      u2.then((fn) => fn());
      u3.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    if (status.status === "disconnected" || status.status === "failed") {
      setBackend(null);
    }
  }, [status.status]);

  useEffect(() => {
    if (!keyPath) { setKeyValid(null); setKeyChecking(false); return; }
    setKeyChecking(true);
    invoke<boolean>("validate_key_path", { path: keyPath })
      .then(() => setKeyValid(true))
      .catch(() => setKeyValid(false))
      .finally(() => setKeyChecking(false));
  }, [keyPath]);

  async function pickKey() {
    const selected = await open({ multiple: false });
    if (typeof selected === "string") setKeyPath(selected);
  }

  async function connect() {
    if (!host || !username) { setError(t("connectForm.validateHostUser")); return; }
    if (!password && !keyPath) { setError(t("connectForm.validatePasswordOrKey")); return; }
    setError("");
    try {
      await invoke("ssh_connect", { host, port, username, password, keyPath });
    } catch (e) {
      setStatus({ status: "failed", reason: String(e) });
    }
  }

  async function disconnect() {
    try { await invoke("ssh_disconnect"); } catch (e) { setError(String(e)); }
  }

  async function copyIp(ip: string) {
    try {
      await navigator.clipboard.writeText(ip);
      setCopiedIp(ip);
      setTimeout(() => setCopiedIp(null), 1500);
    } catch {}
  }

  const isConnected = status.status === "polling";
  const isTransitioning = status.status === "connecting" || status.status === "detecting";

  function statusCapsule() {
    switch (status.status) {
      case "disconnected":
        return <span className={`${styles.capsule} ${styles.off}`}>● {t("connectForm.disconnected")}</span>;
      case "connecting":
        return <span className={`${styles.capsule} ${styles.busy}`}>● {t("connectForm.stepConnecting")}</span>;
      case "detecting":
        return <span className={`${styles.capsule} ${styles.busy}`}>● {t("connectForm.stepDetecting")}</span>;
      case "polling":
        return <span className={`${styles.capsule} ${styles.on}`}>● {t("connectForm.connectedTo", { host: status.host })}</span>;
      case "failed":
        return <span className={`${styles.capsule} ${styles.err}`}>● {t("connectForm.failed", { reason: status.reason })}</span>;
    }
  }

  function fmtTime(iso: string) {
    return new Date(iso).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
  }

  return (
    <div className={styles.pane}>
      {/* Status bar */}
      <div className={styles.statusBar}>
        {localIps.length > 0 && (
          <>
            <span className={styles.statusLabel}>{t("connectForm.localIps")}</span>
            {localIps.map((ip) => (
              <button key={ip} className={styles.ipChip} onClick={() => copyIp(ip)} title={t("connectForm.copyIp")}>
                {copiedIp === ip ? t("connectForm.ipCopied") : ip}
              </button>
            ))}
          </>
        )}
      </div>

      {/* Connection card */}
      <div className={styles.card}>
        <div className={styles.cardHeader}>{statusCapsule()}</div>

        {/* Backend info when connected */}
        {isConnected && backend && (
          <div className={styles.backendRow}>
            <span className={styles.backendLabel}>{t("connectForm.backendLabel")}</span>
            <span className={styles.backendValue}>{backend.method}</span>
            {!backend.supports_image && (
              <span className={styles.imageWarn} title={t("connectForm.imageNotSupported", { method: backend.method })}>
                ⚠ no image
              </span>
            )}
          </div>
        )}
        {isConnected && backend?.method === "none" && (
          <div className={styles.noneHint}>{t("connectForm.backendNoneHint")}</div>
        )}

        {/* Last received */}
        {isConnected && (
          <div className={styles.backendRow}>
            <span className={styles.backendLabel}>{t("connectForm.lastReceived")}</span>
            <span className={styles.backendValue}>
              {lastReceived ? fmtTime(lastReceived) : t("connectForm.neverReceived")}
            </span>
          </div>
        )}

        <div className={styles.fields}>
          {/* Host + Port (responsive 2-col grid) */}
          <div className={styles.hostPortRow}>
            <label className={styles.fieldGrow}>
              <span className={styles.label}>{t("connectForm.host")}</span>
              <input
                type="text"
                placeholder={t("connectForm.hostPlaceholder")}
                value={host}
                onChange={(e) => setHost(e.target.value)}
                disabled={isConnected || isTransitioning}
              />
            </label>
            <label className={styles.fieldPort}>
              <span className={styles.label}>{t("connectForm.port")}</span>
              <input
                type="number" min={1} max={65535}
                value={port}
                onChange={(e) => setPort(Number(e.target.value))}
                disabled={isConnected || isTransitioning}
              />
            </label>
          </div>

          {/* Username */}
          <label className={styles.fieldFull}>
            <span className={styles.label}>{t("connectForm.username")}</span>
            <input
              type="text"
              placeholder={t("connectForm.usernamePlaceholder")}
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              disabled={isConnected || isTransitioning}
            />
          </label>

          {/* Password */}
          <label className={styles.fieldFull}>
            <span className={styles.label}>{t("connectForm.password")}</span>
            <input
              type="password"
              placeholder={t("connectForm.passwordPlaceholder")}
              value={password}
              onChange={(e) => { setPassword(e.target.value); if (!savePw) setSavePw(true); }}
              disabled={isConnected || isTransitioning}
            />
          </label>
          <label className={styles.checkRow}>
            <input
              type="checkbox" checked={savePw}
              onChange={(e) => setSavePw(e.target.checked)}
              disabled={!password || isConnected}
            />
            <span>{t("connectForm.rememberPassword")}</span>
          </label>

          {/* Private key */}
          <label className={styles.fieldFull}>
            <span className={styles.label}>{t("connectForm.privateKey")}</span>
            <div className={styles.rowInput}>
              <input
                type="text" readOnly
                placeholder={t("connectForm.keyPlaceholder")}
                value={keyPath}
                disabled={isConnected || isTransitioning}
              />
              <button className={styles.browseBtn} onClick={pickKey} disabled={isConnected || isTransitioning}>
                {t("connectForm.browse")}
              </button>
              {keyPath && !keyChecking && (
                <span className={keyValid ? styles.keyOk : styles.keyBad}>
                  {keyValid ? "✓" : "✗"}
                </span>
              )}
              {keyChecking && <span className={styles.keyChecking}>⋯</span>}
            </div>
            <span className={styles.subLabel}>{t("connectForm.keyLeaveBlank")}</span>
          </label>
        </div>

        {/* Action button */}
        <div className={styles.actions}>
          {isConnected ? (
            <button className={styles.disconnectBtn} onClick={disconnect}>
              {t("connectForm.disconnect")}
            </button>
          ) : (
            <button className={styles.connectBtn} onClick={connect} disabled={isTransitioning}>
              {isTransitioning ? t("connectForm.connecting") : t("connectForm.connect")}
            </button>
          )}
        </div>

        {error && <div className={styles.error}>{error}</div>}
        <p className={styles.hint}>{t("connectForm.note")}</p>
      </div>
    </div>
  );
}
