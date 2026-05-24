import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import type { HistoryItem } from "../types";
import styles from "./HistoryList.module.css";
import { useLanguage } from "../i18n/LanguageContext";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function fmtSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function fmtTime(iso: string): string {
  return new Date(iso).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function fmtImageFormat(mime?: string): string {
  if (!mime) return "PNG";
  switch (mime) {
    case "image/png":  return "PNG";
    case "image/jpeg": return "JPEG";
    case "image/gif":  return "GIF";
    case "image/bmp":  return "BMP";
    case "image/webp": return "WebP";
    default:           return mime.split("/")[1]?.toUpperCase() || "IMG";
  }
}

function dateKey(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleDateString([], { year: "numeric", month: "long", day: "numeric" });
}

function groupByDate(items: HistoryItem[]): { date: string; items: HistoryItem[] }[] {
  const map = new Map<string, HistoryItem[]>();
  for (const item of items) {
    const key = dateKey(item.created_at);
    if (!map.has(key)) map.set(key, []);
    map.get(key)!.push(item);
  }
  return Array.from(map.entries()).map(([date, items]) => ({ date, items }));
}

function matchesQuery(item: HistoryItem, q: string): boolean {
  const lower = q.toLowerCase();
  if (item.preview?.toLowerCase().includes(lower)) return true;
  if (item.full_text?.toLowerCase().includes(lower)) return true;
  return false;
}

// ---------------------------------------------------------------------------
// Image preview modal
// ---------------------------------------------------------------------------

function ImagePreviewModal({
  url,
  size,
  mimeType,
  onClose,
}: {
  url: string;
  size: number;
  mimeType?: string;
  onClose: () => void;
}) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className={styles.previewOverlay} onClick={onClose}>
      <div className={styles.previewBox} onClick={(e) => e.stopPropagation()}>
        <button className={styles.previewClose} onClick={onClose} title="Close">✕</button>
        <img src={url} alt="preview" className={styles.previewImg} />
        <div className={styles.previewMeta}>{fmtSize(size)} {fmtImageFormat(mimeType)}</div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Confirm dialog
// ---------------------------------------------------------------------------

function ConfirmDialog({
  message,
  onConfirm,
  onCancel,
}: {
  message: string;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const { t } = useLanguage();
  return (
    <div className={styles.overlay}>
      <div className={styles.dialog}>
        <p className={styles.dialogMsg}>{message}</p>
        <div className={styles.dialogBtns}>
          <button className={styles.dialogCancel} onClick={onCancel}>{t("history.cancel")}</button>
          <button className={styles.dialogConfirm} onClick={onConfirm}>{t("history.confirmDelete")}</button>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Image thumbnail
// ---------------------------------------------------------------------------

function useThumbUrl(item: HistoryItem): string | null {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    if (item.kind !== "image") return;
    let objectUrl: string | null = null;
    invoke<number[]>("get_item_image", { id: item.id })
      .then((bytes) => {
        if (!bytes.length) return;
        const arr = new Uint8Array(bytes);
        const mime = item.mime_type || "image/png";
        const blob = new Blob([arr], { type: mime });
        objectUrl = URL.createObjectURL(blob);
        setUrl(objectUrl);
      })
      .catch(() => {});
    return () => { if (objectUrl) URL.revokeObjectURL(objectUrl); };
  }, [item.id, item.kind]);
  return url;
}

// ---------------------------------------------------------------------------
// Single item row
// ---------------------------------------------------------------------------

function ItemRow({
  item,
  selected,
  onToggle,
  onDelete,
}: {
  item: HistoryItem;
  selected: boolean;
  onToggle: (id: string) => void;
  onDelete: (id: string) => void;
}) {
  const { t } = useLanguage();
  const [copied, setCopied] = useState(false);
  const [imgCopied, setImgCopied] = useState(false);
  const [previewOpen, setPreviewOpen] = useState(false);
  const thumbUrl = useThumbUrl(item);

  async function copyText() {
    if (item.full_text) {
      await writeText(item.full_text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    }
  }

  async function copyImage() {
    try {
      await invoke("copy_image_to_clipboard", { id: item.id });
      setImgCopied(true);
      setTimeout(() => setImgCopied(false), 1500);
    } catch {}
  }

  const badge = item.kind === "text" ? t("history.badgeText") : t("history.badgeImage");

  return (
    <li className={`${styles.row} ${selected ? styles.rowSelected : ""}`}>
      <input
        type="checkbox"
        className={styles.check}
        checked={selected}
        onChange={() => onToggle(item.id)}
      />
      <span className={`${styles.badge} ${styles["badge_" + item.kind]}`}>
        {badge}
      </span>
      <div className={styles.info}>
        {item.kind === "text" && (
          <>
            <div className={styles.preview}>
              {item.preview}{(item.full_text?.length ?? 0) > 200 ? "…" : ""}
            </div>
            <div className={styles.meta}>
              {fmtSize(item.size)} · {fmtTime(item.created_at)}
            </div>
          </>
        )}
        {item.kind === "image" && (
          <>
            {thumbUrl ? (
              <button
                className={styles.thumbBtn}
                onClick={() => setPreviewOpen(true)}
                title="Click to enlarge"
              >
                <img src={thumbUrl} alt="thumbnail" className={styles.thumb} />
              </button>
            ) : (
              <div className={styles.preview}>{t("history.imageLabel")}</div>
            )}
            <div className={styles.meta}>
              {fmtSize(item.size)} {fmtImageFormat(item.mime_type)} · {fmtTime(item.created_at)}
            </div>
            {previewOpen && thumbUrl && (
              <ImagePreviewModal
                url={thumbUrl}
                size={item.size}
                mimeType={item.mime_type}
                onClose={() => setPreviewOpen(false)}
              />
            )}
          </>
        )}
      </div>
      {item.kind === "text" && (
        <button
          className={`${styles.copyBtn} ${copied ? styles.copyBtnDone : ""}`}
          title={t("history.copyTitle")}
          onClick={copyText}
        >
          {copied ? t("history.copied") : t("history.copyIcon")}
        </button>
      )}
      {item.kind === "image" && (
        <button
          className={`${styles.copyBtn} ${imgCopied ? styles.copyBtnDone : ""}`}
          title={t("history.copyImage")}
          onClick={copyImage}
        >
          {imgCopied ? t("history.copied") : t("history.copyImage")}
        </button>
      )}
      <button className={styles.del} title={t("history.deleteTitle")} onClick={() => onDelete(item.id)}>
        {t("history.deleteIcon")}
      </button>
    </li>
  );
}

// ---------------------------------------------------------------------------
// Day section
// ---------------------------------------------------------------------------

function DaySection({
  date, items, selectedIds, onToggleItem, onDeleteItem, onDeleteDay, onToggleDay,
}: {
  date: string;
  items: HistoryItem[];
  selectedIds: Set<string>;
  onToggleItem: (id: string) => void;
  onDeleteItem: (id: string) => void;
  onDeleteDay: (date: string) => void;
  onToggleDay: (date: string, ids: string[]) => void;
}) {
  const { t } = useLanguage();
  const [collapsed, setCollapsed] = useState(false);
  const dayIds = items.map((i) => i.id);
  const allSelected = dayIds.every((id) => selectedIds.has(id));

  return (
    <div className={styles.daySection}>
      <div className={styles.dayHeader}>
        <button
          className={styles.collapseBtn}
          onClick={() => setCollapsed((c) => !c)}
          title={collapsed ? t("history.expand") : t("history.collapse")}
        >
          {collapsed ? "▶" : "▼"}
        </button>
        <input
          type="checkbox"
          className={styles.check}
          checked={allSelected}
          onChange={() => onToggleDay(date, dayIds)}
          title={t("history.selectDayTitle")}
        />
        <span className={styles.dayLabel}>{date}</span>
        <span className={styles.dayCount}>{t("history.itemCount", { count: items.length })}</span>
        <button
          className={styles.deleteDayBtn}
          onClick={() => onDeleteDay(date)}
          title={t("history.deleteDayTitle")}
        >
          {t("history.deleteDay")}
        </button>
      </div>
      {!collapsed && (
        <ul className={styles.dayItems}>
          {items.map((item) => (
            <ItemRow
              key={item.id}
              item={item}
              selected={selectedIds.has(item.id)}
              onToggle={onToggleItem}
              onDelete={onDeleteItem}
            />
          ))}
        </ul>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

type ConfirmState =
  | { type: "clear" }
  | { type: "item"; id: string }
  | { type: "day"; date: string; ids: string[] }
  | { type: "selected"; ids: string[] }
  | null;

interface Props {
  version: number; // incremented by App whenever new clipboard item arrives
}

export default function HistoryList({ version }: Props) {
  const { t } = useLanguage();
  const [items, setItems] = useState<HistoryItem[]>([]);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [confirm, setConfirm] = useState<ConfirmState>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const latestItems = useRef(items);
  latestItems.current = items;

  // Reload whenever a new item arrives (version bump) or on mount
  useEffect(() => {
    invoke<HistoryItem[]>("get_history").then(setItems).catch(() => {});
  }, [version]);

  function toggleItem(id: string) {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      next.has(id) ? next.delete(id) : next.add(id);
      return next;
    });
  }

  function toggleDay(_date: string, ids: string[]) {
    const allSelected = ids.every((id) => selectedIds.has(id));
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (allSelected) ids.forEach((id) => next.delete(id));
      else ids.forEach((id) => next.add(id));
      return next;
    });
  }

  async function doDeleteItem(id: string) {
    await invoke("delete_item", { id });
    setItems((prev) => prev.filter((i) => i.id !== id));
    setSelectedIds((prev) => { const n = new Set(prev); n.delete(id); return n; });
  }

  async function doDeleteIds(ids: string[]) {
    await invoke("delete_items", { ids });
    const idSet = new Set(ids);
    setItems((prev) => prev.filter((i) => !idSet.has(i.id)));
    setSelectedIds((prev) => { const n = new Set(prev); ids.forEach((id) => n.delete(id)); return n; });
  }

  async function doClearAll() {
    await invoke("clear_history");
    setItems([]);
    setSelectedIds(new Set());
  }

  function handleConfirm() {
    if (!confirm) return;
    if (confirm.type === "clear") doClearAll();
    else if (confirm.type === "item") doDeleteItem(confirm.id);
    else doDeleteIds(confirm.ids);
    setConfirm(null);
  }

  function requestDeleteDay(date: string) {
    const groups = groupByDate(latestItems.current);
    const group = groups.find((g) => g.date === date);
    if (!group) return;
    setConfirm({ type: "day", date, ids: group.items.map((i) => i.id) });
  }

  const q = searchQuery.trim();
  const visibleItems = q ? items.filter((i) => matchesQuery(i, q)) : items;
  const groups = groupByDate(visibleItems);
  const selCount = selectedIds.size;

  function confirmMessage(): string {
    if (!confirm) return "";
    if (confirm.type === "clear") return t("history.confirmClear", { count: items.length });
    if (confirm.type === "item") return t("history.confirmItem");
    if (confirm.type === "day") return t("history.confirmDay", { date: confirm.date });
    return t("history.confirmSelected", { count: confirm.ids.length });
  }

  return (
    <div className={styles.container}>
      {confirm && (
        <ConfirmDialog
          message={confirmMessage()}
          onConfirm={handleConfirm}
          onCancel={() => setConfirm(null)}
        />
      )}

      <div className={styles.toolbar}>
        <input
          type="search"
          className={styles.searchInput}
          placeholder={t("history.searchPlaceholder")}
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
        />
        <span className={styles.count}>
          {t("history.itemCount", { count: visibleItems.length })}
        </span>
        {selCount > 0 && (
          <button
            className={styles.deleteSelBtn}
            onClick={() => setConfirm({ type: "selected", ids: Array.from(selectedIds) })}
          >
            {t("history.deleteSelected", { count: selCount })}
          </button>
        )}
        <button
          className={styles.clearBtn}
          onClick={() => setConfirm({ type: "clear" })}
          disabled={items.length === 0}
        >
          {t("history.clearAll")}
        </button>
      </div>

      {visibleItems.length === 0 ? (
        <div className={styles.empty}>
          {q ? t("history.noResults") : t("history.empty")}
        </div>
      ) : (
        <div className={styles.list}>
          {groups.map(({ date, items: dayItems }) => (
            <DaySection
              key={date}
              date={date}
              items={dayItems}
              selectedIds={selectedIds}
              onToggleItem={toggleItem}
              onDeleteItem={(id) => setConfirm({ type: "item", id })}
              onDeleteDay={requestDeleteDay}
              onToggleDay={toggleDay}
            />
          ))}
        </div>
      )}
    </div>
  );
}
