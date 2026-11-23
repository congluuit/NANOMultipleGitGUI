import { describe, expect, it } from "vitest";
import { parseInline, parseMarkdown, slug } from "./markdown";

describe("parseMarkdown", () => {
  it("reads headings with unique anchors, paragraphs and rules", () => {
    const blocks = parseMarkdown(
      "# Getting started\r\n\r\nSome *text*\r\nwrapped.\r\n\r\n---\r\n\r\n## Getting started\r\nSetext\r\n======\r\n",
    );
    expect(blocks).toEqual([
      { t: "heading", level: 1, id: "getting-started", c: [{ t: "text", v: "Getting started" }] },
      {
        t: "p",
        c: [
          { t: "text", v: "Some " },
          { t: "em", c: [{ t: "text", v: "text" }] },
          { t: "text", v: " wrapped." },
        ],
      },
      { t: "hr" },
      { t: "heading", level: 2, id: "getting-started-1", c: [{ t: "text", v: "Getting started" }] },
      { t: "heading", level: 1, id: "setext", c: [{ t: "text", v: "Setext" }] },
    ]);
  });

  it("keeps fenced and indented code as it is", () => {
    const blocks = parseMarkdown("```csharp\nvar x = *1*;\n\n<b>\n```\n\n    indented\n    code\n");
    expect(blocks).toEqual([
      { t: "code", lang: "csharp", v: "var x = *1*;\n\n<b>" },
      { t: "code", lang: "", v: "indented\ncode" },
    ]);
  });

  it("nests lists and reads task items", () => {
    const [list] = parseMarkdown("- one\n  - inner\n- [x] done\n- [ ] todo\n\n3. three\n4. four\n");
    expect(list).toMatchObject({ t: "list", ordered: false, tight: true });
    if (list?.t !== "list") throw new Error("not a list");
    expect(list.items).toHaveLength(3);
    expect(list.items[0]?.c[1]).toMatchObject({ t: "list", items: [{ checked: null }] });
    expect(list.items.map((i) => i.checked)).toEqual([null, true, false]);
    const ordered = parseMarkdown("3. three\n4. four\n")[0];
    expect(ordered).toMatchObject({ t: "list", ordered: true, start: 3 });
  });

  it("reads quotes and GitHub tables", () => {
    const blocks = parseMarkdown(
      "> quoted\nlazy line\n\n| Name | Size |\n| :--- | ---: |\n| a \\| b | `1|2` |\n",
    );
    expect(blocks[0]).toMatchObject({ t: "quote", c: [{ t: "p" }] });
    expect(blocks[1]).toEqual({
      t: "table",
      align: ["left", "right"],
      head: [[{ t: "text", v: "Name" }], [{ t: "text", v: "Size" }]],
      rows: [[[{ t: "text", v: "a | b" }], [{ t: "code", v: "1|2" }]]],
    });
  });

  it("keeps raw HTML as a text block", () => {
    const blocks = parseMarkdown(
      '<script>alert("x")</script>\n<img src=x onerror=alert(1)>\n\ntext',
    );
    expect(blocks[0]).toEqual({
      t: "html",
      v: '<script>alert("x")</script>\n<img src=x onerror=alert(1)>',
    });
    expect(blocks[1]).toEqual({ t: "p", c: [{ t: "text", v: "text" }] });
  });
});

describe("parseInline", () => {
  it("reads code, emphasis, links, images and escapes", () => {
    expect(
      parseInline('**bold** _em_ ~~del~~ `a*b` \\*lit\\* [docs](docs/a.md "t") ![logo](l.png)'),
    ).toEqual([
      { t: "strong", c: [{ t: "text", v: "bold" }] },
      { t: "text", v: " " },
      { t: "em", c: [{ t: "text", v: "em" }] },
      { t: "text", v: " " },
      { t: "del", c: [{ t: "text", v: "del" }] },
      { t: "text", v: " " },
      { t: "code", v: "a*b" },
      { t: "text", v: " *lit* " },
      { t: "link", href: "docs/a.md", c: [{ t: "text", v: "docs" }] },
      { t: "text", v: " " },
      { t: "image", alt: "logo", src: "l.png" },
    ]);
  });

  it("links bare and angle URLs, leaves snake_case alone", () => {
    expect(parseInline("see https://example.com/a_b. and <https://x.dev> my_var_name")).toEqual([
      { t: "text", v: "see " },
      {
        t: "link",
        href: "https://example.com/a_b",
        c: [{ t: "text", v: "https://example.com/a_b" }],
      },
      { t: "text", v: ". and " },
      { t: "link", href: "https://x.dev", c: [{ t: "text", v: "https://x.dev" }] },
      { t: "text", v: " my_var_name" },
    ]);
  });

  it("turns two trailing spaces into a line break", () => {
    expect(parseInline("a  \nb\nc")).toEqual([
      { t: "text", v: "a" },
      { t: "br" },
      { t: "text", v: "b c" },
    ]);
  });
});

it("slugs headings like GitHub", () => {
  expect(slug("Hello, World! (v2)")).toBe("hello-world-v2");
  expect(slug("Tiếng Việt")).toBe("tiếng-việt");
});
