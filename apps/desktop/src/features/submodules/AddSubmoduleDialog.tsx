import { useEffect, useState } from "react";
import { LoaderCircle } from "lucide-react";
import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
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
import { commands } from "@/ipc";
import { useAccounts } from "@/features/accounts/queries";
import { addSubmodule } from "./actions";
import { accountNoteKey, pathFromUrl } from "./model";

const inputClass =
  "h-8 rounded-md border bg-background px-2 text-xs focus:border-primary focus:outline-none";

/** "Add submodule…": URL, folder (defaults to the repo name) and optional branch. */
export function AddSubmoduleDialog({
  repoId,
  open,
  onClose,
}: {
  repoId: number;
  open: boolean;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [url, setUrl] = useState("");
  const [path, setPath] = useState("");
  const [pathEdited, setPathEdited] = useState(false);
  const [branch, setBranch] = useState("");
  const [busy, setBusy] = useState(false);
  const [debounced, setDebounced] = useState("");
  const accounts = useAccounts();

  useEffect(() => {
    const id = setTimeout(() => setDebounced(url.trim()), 300);
    return () => clearTimeout(id);
  }, [url]);

  const who = useQuery({
    queryKey: ["submoduleUrlAccount", repoId, debounced],
    queryFn: () => commands.submoduleUrlAccount(repoId, debounced),
    enabled: isTauri() && open && debounced.length > 0,
    staleTime: 30_000,
  });
  const accountName = (() => {
    const a = accounts.data?.find((x) => x.id === who.data?.accountId);
    return a ? (a.label ?? a.username) : t("submodule.noAccount");
  })();

  const effectivePath = pathEdited ? path : pathFromUrl(url);
  const valid = url.trim().length > 0 && effectivePath.trim().length > 0;

  const submit = async () => {
    setBusy(true);
    const ok = await addSubmodule(repoId, url.trim(), effectivePath.trim(), branch.trim() || null);
    setBusy(false);
    if (ok) onClose();
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-lg">
        <form
          className="grid gap-3"
          onSubmit={(e) => {
            e.preventDefault();
            if (valid && !busy) void submit();
          }}
        >
          <DialogHeader>
            <DialogTitle>{t("submodule.addTitle")}</DialogTitle>
            <DialogDescription>{t("submodule.addDesc")}</DialogDescription>
          </DialogHeader>
          <label className="grid gap-1 text-xs">
            <span className="text-muted-foreground">{t("submodule.url")}</span>
            <input
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder="https://github.com/org/lib.git"
              spellCheck={false}
              autoFocus
              className={inputClass}
            />
          </label>
          {who.data && (
            <p className="text-[11px] text-muted-foreground">
              {t(`submodule.account.${accountNoteKey(who.data)}`, { account: accountName })}
            </p>
          )}
          <label className="grid gap-1 text-xs">
            <span className="text-muted-foreground">{t("submodule.path")}</span>
            <input
              value={effectivePath}
              onChange={(e) => {
                setPath(e.target.value);
                setPathEdited(true);
              }}
              placeholder="libs/lib"
              spellCheck={false}
              className={inputClass}
            />
          </label>
          <label className="grid gap-1 text-xs">
            <span className="text-muted-foreground">{t("submodule.branch")}</span>
            <input
              value={branch}
              onChange={(e) => setBranch(e.target.value)}
              placeholder={t("submodule.branchPlaceholder")}
              spellCheck={false}
              className={inputClass}
            />
          </label>
          <DialogFooter>
            <Button type="button" variant="outline" size="sm" onClick={onClose}>
              {t("common.cancel")}
            </Button>
            <Button type="submit" size="sm" disabled={!valid || busy}>
              {busy && <LoaderCircle className="animate-spin" />}
              {t("submodule.addSubmit")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
