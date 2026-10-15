import type { Account, Identity } from "@/ipc";
import { contrastRatio } from "@/theme/contrast";

/**
 * Author avatars on the commit graph (PLAN 5.2).
 *
 * Sources, in order: GitHub noreply addresses → the GitHub avatar of that user; an email of a
 * configured account → the account's avatar; anything else → Gravatar (`d=404`, so a missing
 * Gravatar falls through to the initials). While an image loads, or when every source failed,
 * the graph draws an initials badge instead.
 */

const AVATAR_SIZE = 64;

/** GitHub avatar for `ID+login@users.noreply.github.com` / `login@users.noreply.github.com`. */
export function githubNoreplyAvatar(email: string): string | null {
  const e = email.trim().toLowerCase();
  const withId = /^(\d+)\+([^@]+)@users\.noreply\.github\.com$/.exec(e);
  if (withId) return `https://avatars.githubusercontent.com/u/${withId[1]}?s=${AVATAR_SIZE}&v=4`;
  const plain = /^([^@+]+)@users\.noreply\.github\.com$/.exec(e);
  if (plain) return `https://github.com/${encodeURIComponent(plain[1]!)}.png?size=${AVATAR_SIZE}`;
  return null;
}

export function gravatarUrl(email: string): string {
  return `https://www.gravatar.com/avatar/${md5(email.trim().toLowerCase())}?s=${AVATAR_SIZE}&d=404`;
}

/** Lowercased email → avatar URL of the account it belongs to (from the account identities). */
export function accountAvatarMap(
  accounts: readonly Pick<Account, "id" | "avatarUrl">[] | undefined,
  identities: readonly Pick<Identity, "accountId" | "email">[] | undefined,
): Map<string, string> {
  const map = new Map<string, string>();
  if (!accounts || !identities) return map;
  const byId = new Map(accounts.map((a) => [a.id, a.avatarUrl]));
  for (const identity of identities) {
    const url = byId.get(identity.accountId);
    const email = identity.email.trim().toLowerCase();
    if (url && email && !map.has(email)) map.set(email, url);
  }
  return map;
}

/** Avatar URLs to try for `email`, best first (empty for something that is not an email). */
export function avatarSources(email: string, accounts: ReadonlyMap<string, string>): string[] {
  const e = email.trim().toLowerCase();
  if (!e.includes("@")) return [];
  const out: string[] = [];
  const github = githubNoreplyAvatar(e);
  if (github) out.push(github);
  const account = accounts.get(e);
  if (account && !out.includes(account)) out.push(account);
  out.push(gravatarUrl(e));
  return out;
}

/** One or two initials: first letters of the first and last word of the name (or the email). */
export function initials(name: string, email: string): string {
  const words = name
    .trim()
    .split(/\s+/)
    .filter((w) => /[\p{L}\p{N}]/u.test(w));
  const first = (w: string) => Array.from(w.replace(/^[^\p{L}\p{N}]+/u, ""))[0] ?? "";
  if (words.length === 0) return first(email.trim()).toUpperCase() || "?";
  if (words.length === 1) return first(words[0]!).toUpperCase();
  return (first(words[0]!) + first(words[words.length - 1]!)).toUpperCase();
}

/** Stable small hash (FNV-1a) of a string. */
export function hashString(s: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

export const DARK_TEXT = "#1f1b16";
export const LIGHT_TEXT = "#ffffff";
const contrastCache = new Map<string, string>();

/** Dark or white text, whichever reads better on `bg` (`#rrggbb`; white for anything else). */
export function contrastText(bg: string): string {
  let out = contrastCache.get(bg);
  if (out === undefined) {
    try {
      out = contrastRatio(bg, DARK_TEXT) > contrastRatio(bg, LIGHT_TEXT) ? DARK_TEXT : LIGHT_TEXT;
    } catch {
      out = LIGHT_TEXT;
    }
    if (contrastCache.size > 256) contrastCache.clear();
    contrastCache.set(bg, out);
  }
  return out;
}

export interface InitialsBadge {
  text: string;
  /** Fill, picked from the lane palette by a hash of the email. */
  color: string;
  /** Text color on `color`. */
  fg: string;
}

const badgeCache = new Map<string, InitialsBadge>();
let badgeLanes: readonly string[] | null = null;

/** Initials badge of an author (cached per email; reset when the palette changes). */
export function initialsBadge(
  name: string,
  email: string,
  lanes: readonly string[],
): InitialsBadge {
  if (lanes !== badgeLanes || badgeCache.size > 4096) {
    badgeCache.clear();
    badgeLanes = lanes;
  }
  const key = email || name;
  let badge = badgeCache.get(key);
  if (!badge) {
    const color =
      lanes[hashString(key.trim().toLowerCase()) % Math.max(1, lanes.length)] ?? "#888888";
    badge = { text: initials(name, email), color, fg: contrastText(color) };
    badgeCache.set(key, badge);
  }
  return badge;
}

// ---------------------------------------------------------------------------------------------
// MD5 (Gravatar hashes). Small, synchronous; `crypto.subtle` has no MD5.

const SHIFTS = [7, 12, 17, 22, 5, 9, 14, 20, 4, 11, 16, 23, 6, 10, 15, 21];
const K = Array.from({ length: 64 }, (_, i) => Math.floor(Math.abs(Math.sin(i + 1)) * 2 ** 32) | 0);

/** Hex MD5 of the UTF-8 bytes of `input`. */
export function md5(input: string): string {
  const bytes = new TextEncoder().encode(input);
  const len = bytes.length;
  const padded = new Uint8Array((((len + 8) >>> 6) + 1) * 64);
  padded.set(bytes);
  padded[len] = 0x80;
  const view = new DataView(padded.buffer);
  view.setUint32(padded.length - 8, (len << 3) >>> 0, true);
  view.setUint32(padded.length - 4, Math.floor(len / 2 ** 29), true);

  let a0 = 0x67452301;
  let b0 = 0xefcdab89 | 0;
  let c0 = 0x98badcfe | 0;
  let d0 = 0x10325476;
  const m = new Int32Array(16);
  for (let off = 0; off < padded.length; off += 64) {
    for (let j = 0; j < 16; j++) m[j] = view.getInt32(off + j * 4, true);
    let a = a0;
    let b = b0;
    let c = c0;
    let d = d0;
    for (let i = 0; i < 64; i++) {
      let f: number;
      let g: number;
      if (i < 16) {
        f = (b & c) | (~b & d);
        g = i;
      } else if (i < 32) {
        f = (d & b) | (~d & c);
        g = (5 * i + 1) % 16;
      } else if (i < 48) {
        f = b ^ c ^ d;
        g = (3 * i + 5) % 16;
      } else {
        f = c ^ (b | ~d);
        g = (7 * i) % 16;
      }
      f = (f + a + K[i]! + m[g]!) | 0;
      const s = SHIFTS[(i >> 4) * 4 + (i % 4)]!;
      a = d;
      d = c;
      c = b;
      b = (b + ((f << s) | (f >>> (32 - s)))) | 0;
    }
    a0 = (a0 + a) | 0;
    b0 = (b0 + b) | 0;
    c0 = (c0 + c) | 0;
    d0 = (d0 + d) | 0;
  }
  let hex = "";
  for (const word of [a0, b0, c0, d0]) {
    for (let k = 0; k < 4; k++) hex += ((word >>> (k * 8)) & 0xff).toString(16).padStart(2, "0");
  }
  return hex;
}

// ---------------------------------------------------------------------------------------------
// Image cache.

type EntryState = "idle" | "queued" | "loading" | "ready" | "failed";

interface Entry {
  key: string;
  sources: string[];
  /** Index of the source being tried. */
  next: number;
  state: EntryState;
  img: HTMLImageElement | null;
}

export interface AvatarCacheOptions {
  /** Called when an image became ready (schedule a redraw). */
  onLoad: () => void;
  /** Images loading at once. */
  maxConcurrent?: number;
  /** Authors remembered (oldest are forgotten first). */
  maxEntries?: number;
  /** Waiting loads; the oldest requests (rows scrolled past) are dropped. */
  maxQueue?: number;
  createImage?: () => HTMLImageElement;
}

/**
 * Loads each avatar URL once, remembers failures, falls back to the next source and calls
 * `onLoad` when an image is ready. `get` is cheap enough to call for every visible row on
 * every frame: one map lookup once the author is known.
 */
export class AvatarCache {
  private entries = new Map<string, Entry>();
  private loaded = new Map<string, HTMLImageElement>();
  private failed = new Set<string>();
  /** LIFO: the most recently requested (= visible) rows load first. */
  private queue: Entry[] = [];
  private active = 0;
  private accounts: ReadonlyMap<string, string> = new Map();
  private enabled = true;
  private readonly onLoad: () => void;
  private readonly maxConcurrent: number;
  private readonly maxEntries: number;
  private readonly maxQueue: number;
  private readonly createImage: () => HTMLImageElement;

  constructor(opts: AvatarCacheOptions) {
    this.onLoad = opts.onLoad;
    this.maxConcurrent = opts.maxConcurrent ?? 6;
    this.maxEntries = opts.maxEntries ?? 512;
    this.maxQueue = opts.maxQueue ?? 48;
    this.createImage = opts.createImage ?? (() => new Image());
  }

  /** Off = no network loads, `get` always returns `null` (initials only). */
  setEnabled(on: boolean) {
    this.enabled = on;
    if (!on) this.queue.length = 0;
  }

  /** Account avatars by lowercased email; authors are re-resolved when the map changes. */
  setAccountAvatars(map: ReadonlyMap<string, string>) {
    if (sameMap(map, this.accounts)) return;
    this.accounts = map;
    this.entries.clear();
    this.queue.length = 0;
  }

  /** Loaded avatar of `email`, or `null` (still loading, failed or disabled). */
  get(email: string): HTMLImageElement | null {
    if (!this.enabled || !email) return null;
    let entry = this.entries.get(email);
    if (!entry) {
      entry = {
        key: email,
        sources: avatarSources(email, this.accounts),
        next: 0,
        state: "idle",
        img: null,
      };
      this.entries.set(email, entry);
      if (this.entries.size > this.maxEntries) {
        const oldest = this.entries.keys().next().value;
        if (oldest !== undefined) this.entries.delete(oldest);
      }
    }
    if (entry.state === "ready") return entry.img;
    if (entry.state === "idle") {
      entry.state = "queued";
      this.queue.push(entry);
      if (this.queue.length > this.maxQueue) this.queue.shift()!.state = "idle";
      this.pump();
    }
    return null;
  }

  private current(entry: Entry) {
    return this.entries.get(entry.key) === entry;
  }

  private pump() {
    while (this.active < this.maxConcurrent && this.queue.length > 0) {
      const entry = this.queue.pop()!;
      if (entry.state === "queued" && this.current(entry)) this.start(entry);
    }
  }

  private start(entry: Entry) {
    while (entry.next < entry.sources.length && this.failed.has(entry.sources[entry.next]!)) {
      entry.next++;
    }
    const url = entry.sources[entry.next];
    if (url === undefined) {
      entry.state = "failed";
      return;
    }
    const hit = this.loaded.get(url);
    if (hit) {
      entry.img = hit;
      entry.state = "ready";
      this.onLoad();
      return;
    }
    entry.state = "loading";
    this.active++;
    const img = this.createImage();
    img.decoding = "async";
    img.referrerPolicy = "no-referrer";
    const done = (ok: boolean) => {
      img.onload = null;
      img.onerror = null;
      this.active--;
      if (ok) {
        this.loaded.set(url, img);
        if (this.loaded.size > this.maxEntries) {
          const oldest = this.loaded.keys().next().value;
          if (oldest !== undefined) this.loaded.delete(oldest);
        }
        if (this.current(entry)) {
          entry.img = img;
          entry.state = "ready";
          this.onLoad();
        }
      } else {
        if (this.failed.size > 8192) this.failed.clear();
        this.failed.add(url);
        entry.next++;
        if (this.current(entry)) this.start(entry);
      }
      this.pump();
    };
    img.onload = () => done(img.naturalWidth > 0);
    img.onerror = () => done(false);
    img.src = url;
  }
}

function sameMap(a: ReadonlyMap<string, string>, b: ReadonlyMap<string, string>) {
  if (a.size !== b.size) return false;
  for (const [k, v] of a) if (b.get(k) !== v) return false;
  return true;
}
