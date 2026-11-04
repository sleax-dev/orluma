import { Dropdown, Field, Option } from "@fluentui/react-components";
import { useTranslation } from "react-i18next";

/** Single files bigger than this are left out of file backups (disk images, ISOs, old dumps: rarely what is meant,
 *  and they slow every run). Presets stop at 1 GB (1000 MiB); a larger value an older job already has is kept and shown. */
export const FILE_LIMITS = [100, 250, 500, 1000];
export const DEFAULT_FILE_LIMIT = 500;

export function FileLimit({ value, onChange, className }: { value: number; onChange: (mib: number) => void; className?: string }) {
  const { t } = useTranslation();
  const label = (n: number) => (n === 0 ? t("advanced.noLimit") : n % 1000 === 0 ? `${n / 1000} GB` : `${n} MB`);
  const choices = FILE_LIMITS.includes(value) || value === 0 ? FILE_LIMITS : [...FILE_LIMITS, value].sort((a, b) => a - b);
  return (
    <Field label={t("advanced.largerThan")} hint={t("advanced.largerThanHint")} className={className}>
      <Dropdown value={label(value)} selectedOptions={[String(value)]} onOptionSelect={(_, d) => onChange(Number(d.optionValue))}>
        {[...choices, 0].map((n) => (
          <Option key={n} value={String(n)}>{label(n)}</Option>
        ))}
      </Dropdown>
    </Field>
  );
}
