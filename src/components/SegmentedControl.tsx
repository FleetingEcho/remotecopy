import styles from "./SegmentedControl.module.css";

interface Option {
  label: string;
  value: string;
}

interface Props {
  options: Option[];
  value: string;
  onChange: (v: string) => void;
}

export default function SegmentedControl({ options, value, onChange }: Props) {
  return (
    <div className={styles.control}>
      {options.map((opt) => (
        <button
          key={opt.value}
          className={`${styles.btn} ${value === opt.value ? styles.active : ""}`}
          onClick={() => onChange(opt.value)}
        >
          {opt.label}
        </button>
      ))}
    </div>
  );
}
