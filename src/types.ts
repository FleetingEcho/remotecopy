export interface HistoryItem {
  id: string;
  kind: "text" | "image";
  preview: string;
  full_text?: string;
  size: number;
  created_at: string;
  mime_type?: string; // "image/png", "image/jpeg", etc.
}

export interface SshSettings {
  host: string;
  port: number;
  username: string;
  key_path: string;
  password: string;
  last_connected: string | null;
}

export interface Settings {
  allow_text: boolean;
  allow_image: boolean;
  start_with_windows: boolean;
  max_payload_mb: number;
  history_retention_days: number;
  db_path: string;
  /** "auto" | "light" | "dark" */
  theme: string;
  /** "en" | "zh" */
  language: string;
  ssh: SshSettings;
}
