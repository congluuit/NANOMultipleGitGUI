import { guideEn } from "./en";
import type { GuideBlock, GuideTopic } from "./types";
import { guideVi } from "./vi";

export type { GuideBlock, GuideSection, GuideTopic } from "./types";

/** Guide in `language` (English for anything else). */
export function guideFor(language: string): GuideTopic[] {
  return language.toLowerCase().startsWith("vi") ? guideVi : guideEn;
}

/** Lower case without accents: "Chuyển nhánh" matches "chuyen nhanh". */
export function fold(s: string): string {
  return s
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/đ/g, "d")
    .replace(/Đ/g, "D")
    .toLowerCase();
}

function blockText(b: GuideBlock): string {
  switch (b.kind) {
    case "p":
    case "tip":
    case "warn":
      return b.text;
    case "steps":
    case "list":
      return b.items.join(" ");
    case "term":
      return `${b.term} ${b.text}`;
    case "keys":
      return b.rows.map((r) => r.join(" ")).join(" ");
  }
}

/** Everything searchable in a topic, folded. */
export function topicText(topic: GuideTopic): string {
  return fold(
    [
      topic.title,
      topic.summary,
      ...topic.sections.flatMap((s) => [s.title, ...s.blocks.map(blockText)]),
    ].join(" "),
  );
}

/** Topics containing every word of `query` (accent-insensitive); all topics for an empty query. */
export function searchGuide(topics: GuideTopic[], query: string): GuideTopic[] {
  const words = fold(query).split(/\s+/).filter(Boolean);
  if (words.length === 0) return topics;
  return topics.filter((t) => {
    const text = topicText(t);
    return words.every((w) => text.includes(w));
  });
}
