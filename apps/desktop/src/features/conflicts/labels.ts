import type { TFunction } from "i18next";
import type { ConflictContext, ConflictOrigin } from "@/ipc";

export interface SideLabels {
  origin: ConflictOrigin;
  /** Pane titles. */
  ours: string;
  theirs: string;
  /** Short names for buttons ("Take …"). */
  oursShort: string;
  theirsShort: string;
  /** `abc1234 summary` of the commit behind each side. */
  oursDetail: string | null;
  theirsDetail: string | null;
  /** What ours / theirs mean for this operation, for people new to git. */
  explain: string;
}

const short = (oid: string | null | undefined) => (oid ? oid.slice(0, 7) : null);

/** Interpolation values shared by every `conflict.origin.*` text. */
export function labelVars(ctx: ConflictContext | null | undefined) {
  const head = short(ctx?.ours?.oid) ?? "HEAD";
  const commit = short(ctx?.theirs?.oid) ?? "?";
  return {
    branch: ctx?.currentBranch ?? head,
    name: ctx?.theirsName || ctx?.theirs?.refs[0] || commit,
    onto: ctx?.rebaseOnto[0] ?? head,
    mine: ctx?.rebaseBranch ?? "?",
    commit,
  };
}

/** Explains git's "ours" / "theirs" for the operation in progress (reversed during a rebase). */
export function sideLabels(ctx: ConflictContext | null | undefined, t: TFunction): SideLabels {
  const origin: ConflictOrigin = ctx?.origin ?? "unknown";
  const vars = labelVars(ctx);
  const k = `conflict.origin.${origin}` as const;
  const detail = (c: ConflictContext["ours"]) => (c ? `${short(c.oid)} ${c.summary}` : null);
  return {
    origin,
    ours: t(`${k}.ours`, vars),
    theirs: t(`${k}.theirs`, vars),
    oursShort: t(`${k}.oursShort`, vars),
    theirsShort: t(`${k}.theirsShort`, vars),
    oursDetail: detail(ctx?.ours ?? null),
    theirsDetail: detail(ctx?.theirs ?? null),
    explain: t(`${k}.explain`, vars),
  };
}
