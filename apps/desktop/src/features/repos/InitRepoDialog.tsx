import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useUi } from "@/stores/ui";
import { initRepository } from "./actions";
import { isPlausibleBranchName } from "./branchName";

export function InitRepoDialog() {
  const { t } = useTranslation();
  const request = useUi((s) => s.initRequest);
  const requestInit = useUi((s) => s.requestInit);
  const [branch, setBranch] = useState("main");
  const [busy, setBusy] = useState(false);
  const valid = isPlausibleBranchName(branch);

  const submit = async () => {
    if (!request || !valid) return;
    setBusy(true);
    const ok = await initRepository(request.path, branch.trim());
    setBusy(false);
    if (ok) requestInit(null);
  };

  return (
    <Dialog open={request !== null} onOpenChange={(open) => !open && requestInit(null)}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>
            {request?.reason === "notRepository"
              ? t("repos.initNotRepoTitle")
              : t("repos.initNewTitle")}
          </DialogTitle>
          <DialogDescription className="break-all">
            {request?.reason === "notRepository"
              ? t("repos.initNotRepoBody", { path: request?.path })
              : t("repos.initNewBody", { path: request?.path })}
          </DialogDescription>
        </DialogHeader>
        <form
          className="space-y-1.5"
          onSubmit={(e) => {
            e.preventDefault();
            void submit();
          }}
        >
          <label htmlFor="init-branch" className="text-xs font-semibold">
            {t("repos.initialBranch")}
          </label>
          <input
            id="init-branch"
            value={branch}
            onChange={(e) => setBranch(e.target.value)}
            aria-invalid={!valid}
            className="h-8 w-full rounded-md border bg-background px-2 font-mono text-xs focus:border-primary focus:outline-none aria-invalid:border-danger"
          />
          <p className="pt-1 text-[11px] text-muted-foreground">{t("repos.initNextSteps")}</p>
        </form>
        <DialogFooter>
          <Button variant="outline" size="sm" onClick={() => requestInit(null)}>
            {t("common.cancel")}
          </Button>
          <Button size="sm" disabled={!valid || busy} onClick={() => void submit()}>
            {t("repos.create")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
