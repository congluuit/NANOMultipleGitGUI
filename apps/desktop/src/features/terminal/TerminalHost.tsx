import { lazy, Suspense, useEffect } from "react";
import { useActiveRepo, useWorkspace } from "@/stores/workspace";
import { useTerminal } from "./store";

// xterm.js is only downloaded once the first terminal is opened.
const TerminalPanel = lazy(() => import("./TerminalPanel"));

/** Mount point of the terminal panel below the main area (see AppShell). */
export function TerminalHost() {
  const repo = useActiveRepo();
  const used = useTerminal((s) => s.open || s.tabs.length > 0);
  const retainRepos = useTerminal((s) => s.retainRepos);
  const repoTabs = useWorkspace((s) => s.tabs);
  const loaded = useWorkspace((s) => s.loaded);

  // Closing a repo tab ends its terminals.
  useEffect(() => {
    if (loaded) retainRepos(repoTabs.map((tab) => tab.id));
  }, [loaded, repoTabs, retainRepos]);

  if (!used) return null;
  return (
    <Suspense fallback={null}>
      <TerminalPanel repoId={repo && !repo.missing ? repo.id : null} />
    </Suspense>
  );
}
