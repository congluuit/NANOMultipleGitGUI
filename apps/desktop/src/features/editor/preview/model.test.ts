import { describe, expect, it, vi } from "vitest";
import { formatJson, linkTarget, previewKind, svgImageUrl } from "./model";
import { modelTextStore, PREVIEW_DELAY_MS, type TextSource } from "./previewText";

describe("previewKind", () => {
  it("knows Markdown, SVG and JSON files", () => {
    expect(previewKind("README.md")).toBe("markdown");
    expect(previewKind("docs/Guide.MARKDOWN")).toBe("markdown");
    expect(previewKind("Assets/icon.svg")).toBe("svg");
    expect(previewKind("package.json")).toBe("json");
    expect(previewKind("Assets/Scripts/Game.asmdef")).toBe("json");
    expect(previewKind("Assets/Player.cs")).toBeNull();
    expect(previewKind(".md")).toBeNull();
    expect(previewKind("Makefile")).toBeNull();
  });
});

describe("formatJson", () => {
  it("pretty-prints valid JSON, BOM or not", () => {
    expect(formatJson('\u{feff}{"a":[1,{"b":null}]}')).toEqual({
      ok: true,
      pretty: '{\n  "a": [\n    1,\n    {\n      "b": null\n    }\n  ]\n}',
    });
  });

  it("says where invalid JSON breaks", () => {
    const r = formatJson('{\n  "a": 1,\n  oops\n}');
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.line).toBe(3);
    expect(r.column).toBe(3);
    expect(r.message).not.toBe("");
    const empty = formatJson("");
    expect(empty.ok).toBe(false);
  });
});

it("draws an SVG from a data URL, never as markup in the page", () => {
  const url = svgImageUrl('<svg xmlns="http://www.w3.org/2000/svg"><script>x()</script></svg>');
  expect(url.startsWith("data:image/svg+xml;charset=utf-8,")).toBe(true);
  expect(url).not.toContain("<");
});

describe("linkTarget", () => {
  it("opens https pages outside and follows nothing else with a scheme", () => {
    expect(linkTarget("https://unity.com", "README.md")).toEqual({
      kind: "external",
      url: "https://unity.com",
    });
    for (const href of [
      "http://x.dev",
      "javascript:alert(1)",
      "mailto:a@b.c",
      "file:///c:/x",
      "//x",
    ])
      expect(linkTarget(href, "README.md")).toEqual({ kind: "none" });
  });

  it("resolves headings and files of the repository", () => {
    expect(linkTarget("#Set%20up", "a.md")).toEqual({ kind: "anchor", id: "Set up" });
    expect(linkTarget("../Docs/Build.md#steps", "Assets/README.md")).toEqual({
      kind: "file",
      path: "Docs/Build.md",
      anchor: "steps",
    });
    expect(linkTarget("./img/a%20b.png?raw=1", "docs/x.md")).toEqual({
      kind: "file",
      path: "docs/img/a b.png",
      anchor: null,
    });
    expect(linkTarget("/LICENSE", "docs/deep/x.md")).toEqual({
      kind: "file",
      path: "LICENSE",
      anchor: null,
    });
    expect(linkTarget("../../out.md", "docs/x.md")).toEqual({ kind: "none" });
  });
});

describe("modelTextStore", () => {
  function fakeEditor(initial: string) {
    let value = initial;
    const listeners = { content: [] as (() => void)[], model: [] as (() => void)[] };
    const editor: TextSource = {
      getModel: () => ({ getValue: () => value }),
      onDidChangeModelContent: (l) => {
        listeners.content.push(l);
        return { dispose: () => (listeners.content = listeners.content.filter((x) => x !== l)) };
      },
      onDidChangeModel: (l) => {
        listeners.model.push(l);
        return { dispose: () => (listeners.model = listeners.model.filter((x) => x !== l)) };
      },
    };
    const type = (text: string) => {
      value = text;
      listeners.content.forEach((l) => l());
    };
    return { editor, type, listeners };
  }

  it("follows the buffer shortly after each edit", () => {
    vi.useFakeTimers();
    try {
      const { editor, type, listeners } = fakeEditor("# One");
      const store = modelTextStore(editor, true);
      expect(store.get()).toBe("# One");
      const notify = vi.fn();
      const stop = store.subscribe(notify);
      type("# Two");
      type("# Three");
      expect(store.get()).toBe("# One");
      vi.advanceTimersByTime(PREVIEW_DELAY_MS);
      expect(store.get()).toBe("# Three");
      stop();
      expect(listeners.content).toHaveLength(0);
    } finally {
      vi.useRealTimers();
    }
  });

  it("does nothing while the preview is hidden", () => {
    const { editor, listeners } = fakeEditor("x");
    const store = modelTextStore(editor, false);
    store.subscribe(() => {});
    expect(store.get()).toBe("");
    expect(listeners.content).toHaveLength(0);
  });
});
