import { render, screen } from "@testing-library/react";
import i18n from "i18next";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { useRefDialog } from "@/features/commits/store";
import { initI18n } from "@/i18n";
import { commands, type MergeForecast } from "@/ipc";
import { useUi } from "@/stores/ui";
import { checkConflicts, forecastNotice } from "./actions";
import { ForecastView } from "./MergeForecast";

beforeAll(() => {
  initI18n("en");
});

afterEach(() => {
  vi.restoreAllMocks();
  useUi.getState().showNotice(null);
  useRefDialog.getState().open(null);
});

const f = (kind: MergeForecast["kind"], conflicts: string[] = []): MergeForecast => ({
  kind,
  conflicts,
});

describe("ForecastView", () => {
  it("shows the loading line, then nothing for an unknown forecast", () => {
    const { rerender, container } = render(<ForecastView forecast={undefined} loading />);
    expect(screen.getByRole("status")).toHaveTextContent("Checking for conflicts…");
    rerender(<ForecastView forecast={f("unknown")} loading={false} />);
    expect(container).toBeEmptyDOMElement();
    rerender(<ForecastView forecast={null} loading={false} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("says a clean, fast-forward or up-to-date merge in one line", () => {
    const { rerender } = render(<ForecastView forecast={f("clean")} loading={false} />);
    expect(screen.getByRole("status")).toHaveTextContent("Merges cleanly, no conflicts.");
    rerender(<ForecastView forecast={f("fastForward")} loading={false} />);
    expect(screen.getByRole("status")).toHaveTextContent(/^Fast-forward/);
    rerender(<ForecastView forecast={f("upToDate")} loading={false} />);
    expect(screen.getByRole("status")).toHaveTextContent("Already up to date");
  });

  it("lists the conflicting files and folds the rest", () => {
    const files = ["a.txt", "b.txt", "c.txt", "d.txt", "e.txt", "f.txt", "g.txt"];
    render(<ForecastView forecast={f("conflicts", files)} loading={false} />);
    expect(screen.getByText("7 files will conflict:")).toBeInTheDocument();
    expect(screen.getAllByRole("listitem").map((li) => li.textContent)).toEqual(files.slice(0, 5));
    expect(screen.getByText("and 2 more files")).toBeInTheDocument();
  });

  it("labels the rebase forecast as an estimate and leaves trivial cases to the rebase dialog", () => {
    const { rerender, container } = render(
      <ForecastView forecast={f("conflicts", ["x.rs"])} loading={false} estimate />,
    );
    expect(screen.getByText("Estimate: 1 file may conflict:")).toBeInTheDocument();
    expect(screen.getByText(/actual conflicts can differ/)).toBeInTheDocument();
    rerender(<ForecastView forecast={f("clean")} loading={false} estimate />);
    expect(screen.getByRole("status")).toHaveTextContent(/^Estimate: no conflicts expected/);
    rerender(<ForecastView forecast={f("fastForward")} loading={false} estimate />);
    expect(container).toBeEmptyDOMElement();
    rerender(<ForecastView forecast={f("upToDate")} loading={false} estimate />);
    expect(container).toBeEmptyDOMElement();
  });

  it("is translated", async () => {
    await i18n.changeLanguage("vi");
    try {
      render(<ForecastView forecast={f("conflicts", ["a", "b"])} loading={false} />);
      expect(screen.getByText("2 file sẽ bị xung đột:")).toBeInTheDocument();
    } finally {
      await i18n.changeLanguage("en");
    }
  });
});

describe("forecastNotice", () => {
  it("builds a notice for each kind", () => {
    expect(forecastNotice(f("unknown"), "topic", "main")).toMatchObject({ kind: "info" });
    expect(forecastNotice(f("upToDate"), "topic", "main").message).toBe(
      "main already contains topic. Nothing to merge.",
    );
    expect(forecastNotice(f("clean"), "topic", "main")).toMatchObject({
      kind: "success",
      message: "topic merges cleanly into main.",
      actions: [],
    });
    const many = Array.from({ length: 7 }, (_, i) => `f${i}.txt`);
    const n = forecastNotice(f("conflicts", many), "topic", "main", () => {});
    expect(n.kind).toBe("info");
    expect(n.message).toBe("Merging topic into main would conflict in 7 files.");
    expect(n.hint).toBe("f0.txt, f1.txt, f2.txt, f3.txt, f4.txt and 2 more files");
    expect(n.detail).toBe(many.join("\n"));
    expect(n.actions?.map((a) => a.label)).toEqual(["Merge…"]);
  });
});

describe("checkConflicts", () => {
  it("asks for HEAD as the base and offers to open the merge dialog", async () => {
    const spy = vi
      .spyOn(commands, "mergeForecast")
      .mockResolvedValue(f("conflicts", ["README.md"]));
    await checkConflicts(3, "origin/topic", "main");
    expect(spy).toHaveBeenCalledWith(3, "origin/topic", null);
    const notice = useUi.getState().notice!;
    expect(notice.message).toBe("Merging origin/topic into main would conflict in 1 file.");
    expect(notice.repoId).toBe(3);
    notice.actions![0]!.run();
    expect(useRefDialog.getState().dialog).toEqual({
      kind: "merge",
      repoId: 3,
      rev: "origin/topic",
      into: "main",
    });
  });

  it("reports a failed call as an error notice", async () => {
    vi.spyOn(commands, "mergeForecast").mockRejectedValue({
      kind: "notFound",
      message: "repository not open",
    });
    await checkConflicts(3, "topic", "main");
    expect(useUi.getState().notice).toMatchObject({
      kind: "error",
      message: "repository not open",
    });
  });
});
