/** Fixtures for the clone tests. Test-only. */
import type { CloneRepo } from "@/ipc";

export function makeRepo(over: Partial<CloneRepo> & { fullName: string }): CloneRepo {
  const parts = over.fullName.split("/");
  const name = parts.pop() ?? over.fullName;
  return {
    accountId: "work",
    id: over.fullName,
    owner: parts.join("/"),
    name,
    description: null,
    private: false,
    fork: false,
    parentFullName: null,
    archived: false,
    defaultBranch: "main",
    cloneHttps: `https://gitlab.acme.dev/${over.fullName}.git`,
    cloneSsh: `git@gitlab.acme.dev:${over.fullName}.git`,
    webUrl: `https://gitlab.acme.dev/${over.fullName}`,
    updatedAt: 1_000,
    stars: 0,
    sizeBytes: null,
    lfsEnabled: null,
    lfsSizeBytes: null,
    canPush: true,
    ...over,
  };
}
