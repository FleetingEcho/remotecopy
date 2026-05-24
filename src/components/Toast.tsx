import { useEffect, useRef } from "react";
import styles from "./Toast.module.css";

export interface ToastItem {
  id: number;
  kind: "text" | "image";
  message: string;
}

interface Props {
  toasts: ToastItem[];
  onDismiss: (id: number) => void;
}

export default function Toast({ toasts, onDismiss }: Props) {
  return (
    <div className={styles.container}>
      {toasts.map((toast) => (
        <ToastEntry key={toast.id} toast={toast} onDismiss={onDismiss} />
      ))}
    </div>
  );
}

function ToastEntry({ toast, onDismiss }: { toast: ToastItem; onDismiss: (id: number) => void }) {
  const ref = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    ref.current = setTimeout(() => onDismiss(toast.id), 3000);
    return () => { if (ref.current) clearTimeout(ref.current); };
  }, [toast.id, onDismiss]);

  return (
    <div className={`${styles.toast} ${toast.kind === "image" ? styles.image : styles.text}`} onClick={() => onDismiss(toast.id)}>
      <span className={styles.icon}>{toast.kind === "image" ? "🖼️" : "📋"}</span>
      <span className={styles.msg}>{toast.message}</span>
    </div>
  );
}
