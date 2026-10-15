import { describe, expect, it } from "vitest";
import { labelAt, parseRefs, refLabelBoxes, textColumnX } from "./draw";

describe("ref label hit-testing", () => {
  it("lays labels out like the canvas and finds the one under the pointer", () => {
    const labels = parseRefs(["HEAD -> refs/heads/main", "refs/heads/topic", "tag: refs/tags/v1"]);
    // 6 px per character, 12 px padding, 4 px gap.
    const boxes = refLabelBoxes(labels, 100, (t) => t.length * 6);
    expect(boxes.map((b) => [b.label.text, b.x, b.width])).toEqual([
      ["main", 100, 36],
      ["topic", 140, 42],
      ["v1", 186, 24],
    ]);
    expect(labelAt(boxes, 120)?.text).toBe("main");
    expect(labelAt(boxes, 138)).toBeNull(); // the gap
    expect(labelAt(boxes, 150)?.text).toBe("topic");
    expect(labelAt(boxes, 400)).toBeNull();
    expect(textColumnX(1)).toBeGreaterThan(0);
  });
});
