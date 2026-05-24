import styles from "./Toggle.module.css";

interface Props {
  checked: boolean;
  onChange: (v: boolean) => void;
  disabled?: boolean;
}

export default function Toggle({ checked, onChange, disabled = false }: Props) {
  return (
    <button
      role="switch"
      aria-checked={checked}
      disabled={disabled}
      className={`${styles.track} ${checked ? styles.on : ""} ${disabled ? styles.disabled : ""}`}
      onClick={() => !disabled && onChange(!checked)}
    >
      <span className={styles.thumb} />
    </button>
  );
}
