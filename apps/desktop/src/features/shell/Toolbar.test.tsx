import { act, render, screen } from "@testing-library/react";
import { QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, describe, expect, it } from "vitest";
import { TooltipProvider } from "@/components/ui/tooltip";
import { initI18n } from "@/i18n";
import { queryClient } from "@/lib/queryClient";
import { useWorkspace } from "@/stores/workspace";
import { Toolbar } from "./Toolbar";

beforeEach(() => {
  initI18n("en");
  queryClient.clear();
  useWorkspace.setState({
    tabs: [
      { id: 1, path: "D:/Work/api", name: "api", missing: false },
      { id: 2, path: "D:/Work/web", name: "web", missing: false },
    ],
    activeId: 1,
    loaded: true,
  });
});

describe("Toolbar", () => {
  it("shows one set of remote buttons after switching repositories", () => {
    render(
      <QueryClientProvider client={queryClient}>
        <TooltipProvider>
          <Toolbar />
        </TooltipProvider>
      </QueryClientProvider>,
    );
    for (const id of [2, 1, 2]) {
      act(() => useWorkspace.setState({ activeId: id }));
      expect(screen.getAllByRole("button", { name: "Fetch" })).toHaveLength(1);
    }
  });
});
