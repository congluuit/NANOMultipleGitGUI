/**
 * File size limits learned from servers that refused a push ("this exceeds the 10 MiB
 * limit"), kept per host. Push Guard checks the next push against them, so a file the server
 * will refuse is caught before minutes of uploading instead of after.
 *
 * The app's database is where they live (so a backup carries them and clearing the webview
 * data does not lose them); localStorage is only the copy read synchronously here.
 */
import { isTauri } from "@tauri-apps/api/core";
import { remoteHost } from "@/features/lfs/guard";
import { repoKeys } from "@/features/repos/queries";
import { commands, type RepoRefs } from "@/ipc";
import { queryClient } from "@/lib/queryClient";

const KEY = "nanomgg.guard.serverLimits";

type Limits = Record<string, number>;

function load(): Limits {
  try {
    const raw = localStorage.getItem(KEY);
    const parsed: unknown = raw ? JSON.parse(raw) : {};
    return parsed && typeof parsed === "object" ? (parsed as Limits) : {};
  } catch {
    return {};
  }
}

function save(limits: Limits) {
  try {
    localStorage.setItem(KEY, JSON.stringify(limits));
  } catch {
    // Storage off: the limit is simply learned again next time.
  }
}

/** Host of `remote` in a repository (from the cached refs). */
export function remoteHostOf(repoId: number, remote: string): string | null {
  const refs = queryClient.getQueryData<RepoRefs>(repoKeys.refs(repoId));
  const url = refs?.remotes.find((r) => r.name === remote)?.url;
  return url ? remoteHost(url) : null;
}

/** Limit learned for the host of `remote`, if any. */
export function learnedLimit(repoId: number, remote: string): number | null {
  const host = remoteHostOf(repoId, remote);
  if (!host) return null;
  const limit = load()[host];
  return typeof limit === "number" && limit > 0 ? limit : null;
}

/** Remembers the smallest limit a server reported. */
export function learnLimit(repoId: number, remote: string, bytes: number) {
  const host = remoteHostOf(repoId, remote);
  if (!host || !(bytes > 0)) return;
  const limits = load();
  const known = limits[host];
  if (known === undefined || bytes < known) {
    limits[host] = Math.floor(bytes);
    save(limits);
    if (isTauri()) {
      commands
        .serverLimitLearn(host, Math.floor(bytes))
        .catch((e: unknown) => console.debug("server limit not saved", e));
    }
  }
}

/** The smaller limit of each host, from both lists. */
export function mergeLimits(a: Limits, b: Limits): Limits {
  const out: Limits = { ...a };
  for (const [host, bytes] of Object.entries(b)) {
    if (!(bytes > 0)) continue;
    const known = out[host];
    if (known === undefined || bytes < known) out[host] = bytes;
  }
  return out;
}

/**
 * At startup: brings the limits kept in the database into the local copy, and moves the
 * ones an earlier version only kept in localStorage into the database.
 */
export async function syncServerLimits(): Promise<void> {
  if (!isTauri()) return;
  try {
    const stored = await commands.serverLimits();
    const local = load();
    save(mergeLimits(stored, local));
    for (const [host, bytes] of Object.entries(local)) {
      const known = stored[host];
      if (bytes > 0 && (known === undefined || bytes < known)) {
        await commands.serverLimitLearn(host, Math.floor(bytes));
      }
    }
  } catch (e) {
    console.debug("server limits not synced", e);
  }
}

const UNITS: Record<string, number> = {
  b: 1,
  kb: 1000,
  kib: 1024,
  mb: 1000 * 1000,
  mib: 1024 * 1024,
  gb: 1000 * 1000 * 1000,
  gib: 1024 * 1024 * 1024,
};

/**
 * Size limit named in a server's refusal: GitHub "file size limit of 100.00 MB", GitLab
 * "exceed the 100.0MiB limit", Bitbucket "a file that is 100 MB or larger". "MB" is read as
 * 10^6 bytes so a file just under a MiB-based limit is moved too (never one too few).
 */
export function parseServerLimit(text: string): number | null {
  const patterns = [
    /limit of\s*([\d.,]+)\s*([kmg]i?b)\b/i,
    /exceeds? the\s*([\d.,]+)\s*([kmg]i?b)\s*limit/i,
    /file that is\s*([\d.,]+)\s*([kmg]i?b)\s*or larger/i,
    /maximum (?:file )?size(?: of)?\s*([\d.,]+)\s*([kmg]i?b)/i,
  ];
  for (const re of patterns) {
    const m = re.exec(text);
    if (!m) continue;
    const value = Number(m[1]!.replace(",", "."));
    const unit = UNITS[m[2]!.toLowerCase()];
    if (Number.isFinite(value) && value > 0 && unit) return Math.floor(value * unit);
  }
  return null;
}

/** Files a server named in its refusal (GitHub: "File assets/a.psd is 120.00 MB; ..."). */
export function parseRejectedPaths(text: string): string[] {
  const out: string[] = [];
  for (const m of text.matchAll(/File (.+?) is [\d.,]+ [KMG]i?B; this exceeds/g)) {
    const path = m[1]!.trim();
    if (path && !out.includes(path)) out.push(path);
  }
  return out;
}
