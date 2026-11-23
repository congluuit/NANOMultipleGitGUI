import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { initI18n } from "@/i18n";
import { openExternal } from "@/features/accounts/manager/openExternal";
import { FilePreview } from "./FilePreview";

vi.mock("@/features/accounts/manager/openExternal", () => ({
  openExternal: vi.fn(() => Promise.resolve(true)),
}));

beforeEach(() => {
  initI18n("en");
  vi.mocked(openExternal).mockClear();
});

describe("Markdown preview", () => {
  const README = [
    "# Project",
    "",
    'Hello <b onclick="x()">world</b>, see [Unity](https://unity.com), [docs](docs/Build.md),',
    "[evil](javascript:alert(1)) and [top](#project).",
    "",
    "<script>window.hacked = true</script>",
    '<img src="x" onerror="window.hacked = true">',
    "",
    "- [x] done",
    "- [ ] todo",
    "",
    "![screenshot](https://example.com/s.png)",
  ].join("\n");

  it("renders Markdown without ever turning file content into HTML", () => {
    const { container } = render(
      <FilePreview kind="markdown" text={README} path="README.md" onOpenFile={() => {}} />,
    );
    expect(screen.getByRole("heading", { level: 1, name: "Project" })).toBeInTheDocument();
    // Raw HTML is shown as text: no element from the file, no script, no image request.
    expect(container.querySelector("script, img, b, [onclick], [onerror]")).toBeNull();
    expect(screen.getByText(/<script>window.hacked = true<\/script>/)).toBeInTheDocument();
    expect(screen.getByText(/<b onclick="x\(\)">world<\/b>/)).toBeInTheDocument();
    expect((window as { hacked?: boolean }).hacked).toBeUndefined();
    // Task items are read-only checkboxes.
    const boxes = screen.getAllByRole("checkbox");
    expect(boxes.map((b) => (b as HTMLInputElement).checked)).toEqual([true, false]);
    expect(screen.getByText("screenshot")).toBeInTheDocument();
  });

  it("opens web links outside, files in the editor, and never a script link", async () => {
    const user = userEvent.setup();
    const onOpenFile = vi.fn();
    render(<FilePreview kind="markdown" text={README} path="README.md" onOpenFile={onOpenFile} />);

    await user.click(screen.getByText("Unity"));
    expect(openExternal).toHaveBeenCalledWith("https://unity.com");
    await user.click(screen.getByText("docs"));
    expect(onOpenFile).toHaveBeenCalledWith("docs/Build.md");

    const evil = screen.getByText("evil");
    expect(evil.closest("a")?.getAttribute("href")).toBeNull();
    await user.click(evil);
    expect(openExternal).toHaveBeenCalledTimes(1);
    expect(onOpenFile).toHaveBeenCalledTimes(1);
    for (const a of document.querySelectorAll("a")) {
      expect(a.getAttribute("href") ?? "#").toBe("#");
    }
  });

  it("scrolls to a heading of the same file", async () => {
    const scroll = vi.fn();
    Element.prototype.scrollIntoView = scroll;
    render(<FilePreview kind="markdown" text={README} path="README.md" />);
    await userEvent.setup().click(screen.getByText("top"));
    expect(scroll).toHaveBeenCalledTimes(1);
  });

  it("shows the new text when the buffer changes", () => {
    const { rerender } = render(<FilePreview kind="markdown" text="# One" path="a.md" />);
    expect(screen.getByRole("heading", { name: "One" })).toBeInTheDocument();
    rerender(<FilePreview kind="markdown" text="# Two" path="a.md" />);
    expect(screen.getByRole("heading", { name: "Two" })).toBeInTheDocument();
  });
});

describe("SVG preview", () => {
  const SVG = '<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script><rect/></svg>';

  it("draws the SVG through an image, never inlined", () => {
    const { container } = render(<FilePreview kind="svg" text={SVG} path="Assets/icon.svg" />);
    const img = screen.getByTestId("svg-preview");
    expect(img.tagName).toBe("IMG");
    expect(img.getAttribute("src")).toMatch(/^data:image\/svg\+xml;charset=utf-8,/);
    expect(img).toHaveAttribute("alt", "icon.svg");
    expect(container.querySelector("svg, script, rect")).toBeNull();
  });

  it("says so when the SVG cannot be drawn, and tries again after an edit", () => {
    const { rerender } = render(<FilePreview kind="svg" text="<svg" path="a.svg" />);
    fireEvent.error(screen.getByTestId("svg-preview"));
    expect(screen.getByRole("alert")).toHaveTextContent("This SVG cannot be drawn");
    rerender(<FilePreview kind="svg" text={SVG} path="a.svg" />);
    expect(screen.getByTestId("svg-preview")).toBeInTheDocument();
  });
});

describe("JSON preview", () => {
  it("pretty-prints valid JSON", () => {
    render(<FilePreview kind="json" text='{"name":"Game","refs":["Core"]}' path="Game.asmdef" />);
    expect(screen.getByTestId("json-preview").textContent).toBe(
      '{\n  "name": "Game",\n  "refs": [\n    "Core"\n  ]\n}',
    );
  });

  it("explains invalid JSON with its line and column", () => {
    initI18n("vi");
    render(<FilePreview kind="json" text={'{\n  "a": 1,\n}'} path="a.json" />);
    expect(screen.getByRole("alert")).toHaveTextContent("JSON không hợp lệ: dòng 3, cột 1.");
  });
});
