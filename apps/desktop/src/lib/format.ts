const UNITS = ["B", "KB", "MB", "GB", "TB"] as const;

/** Formats a byte count with binary multiples (1 KB = 1024 B), e.g. `312 MB`, `1.8 GB`. */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "–";
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit++;
  }
  const digits = unit === 0 || value >= 100 ? 0 : value >= 10 ? 1 : 2;
  return `${Number(value.toFixed(digits))} ${UNITS[unit]}`;
}

export const LARGE_FILE_WARN_BYTES = 50 * 1024 * 1024;
