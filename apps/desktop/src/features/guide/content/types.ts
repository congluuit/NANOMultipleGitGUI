/**
 * Phase 6.7: structure of the Full Guide. Each language has its own module (`en.ts`, `vi.ts`)
 * with exactly the same topics, sections and block kinds (checked by `content.test.ts`).
 */
export type GuideBlock =
  /** A paragraph. */
  | { kind: "p"; text: string }
  /** Numbered "click here, then there" steps. */
  | { kind: "steps"; items: string[] }
  /** Bullet list. */
  | { kind: "list"; items: string[] }
  /** A git word explained in one everyday sentence (shown as a small card). */
  | { kind: "term"; term: string; text: string }
  /** Good to know / be careful. */
  | { kind: "tip"; text: string }
  | { kind: "warn"; text: string }
  /** Keyboard shortcuts: keys → what they do. */
  | { kind: "keys"; rows: [keys: string, action: string][] };

export interface GuideSection {
  id: string;
  title: string;
  blocks: GuideBlock[];
}

export interface GuideTopic {
  id: string;
  title: string;
  /** One line shown under the title and used by the search. */
  summary: string;
  sections: GuideSection[];
}
