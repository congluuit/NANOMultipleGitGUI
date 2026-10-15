import { useCallback } from "react";
import { CircleCheck, CircleDot, CircleMinus, CircleX, type LucideIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useAccounts } from "@/features/accounts/queries";
import { accountLabel } from "@/features/accounts/switcher/model";
import type { CiJob, CiJobState, CiState } from "@/ipc";
import { countJobs } from "./model";

/** Icon of an overall state (`null` for "no CI"). */
export const STATE_ICONS: Record<CiState, LucideIcon | null> = {
  success: CircleCheck,
  failure: CircleX,
  pending: CircleDot,
  neutral: CircleMinus,
  none: null,
};

/** Icon of a job state. */
export function jobIcon(state: CiJobState): LucideIcon {
  switch (state) {
    case "success":
      return CircleCheck;
    case "failure":
    case "cancelled":
      return CircleX;
    case "queued":
    case "running":
      return CircleDot;
    default:
      return CircleMinus;
  }
}

/** "CI: thành công · 3/4 job đạt" — tooltip text of a state. */
export function useCiTooltip() {
  const { t } = useTranslation();
  return useCallback(
    (state: CiState, jobs?: readonly CiJob[]) => {
      const base = t(`ci.state.${state}`);
      if (!jobs || jobs.length === 0) return base;
      const c = countJobs(jobs);
      return `${base} · ${t("ci.jobCounts", { passed: c.success, total: c.total })}`;
    },
    [t],
  );
}

/** Display label of an account id (the id itself while accounts load). */
export function useAccountLabelOf() {
  const accounts = useAccounts().data;
  return useCallback(
    (id: string) => {
      const a = accounts?.find((x) => x.id === id);
      return a ? accountLabel(a) : id;
    },
    [accounts],
  );
}
