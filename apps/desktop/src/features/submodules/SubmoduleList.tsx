import { useState } from "react";
import {
  CircleAlert,
  Copy,
  Download,
  FolderOpen,
  Layers,
  Link2,
  LoaderCircle,
  Plus,
  RefreshCw,
  Trash2,
  Unplug,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Button } from "@/components/ui/button";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { errorMessage, type SubmoduleInfo } from "@/ipc";
import { useAccounts } from "@/features/accounts/queries";
import { cn } from "@/lib/utils";
import { openSubmodule, removeSubmodule, syncSubmodules, updateSubmodules } from "./actions";
import { AddSubmoduleDialog } from "./AddSubmoduleDialog";
import { accountNoteKey, baseName, needsInit, short, submoduleStatus } from "./model";
import { useSubmoduleChangeEvents, useSubmodules } from "./queries";

const TONE: Record<string, string> = {
  ok: "text-success",
  warning: "text-warning",
  danger: "text-danger",
  muted: "text-muted-foreground",
};

type Removal = { info: SubmoduleInfo; mode: "deinit" | "remove"; force: boolean };

/** Rows of the sidebar "Submodules" section with a context menu per submodule. */
export function SubmoduleList({
  repoId,
  root,
  filter = "",
}: {
  repoId: number;
  root: string;
  filter?: string;
}) {
  const { t } = useTranslation();
  const list = useSubmodules(repoId);
  useSubmoduleChangeEvents(repoId);
  const accounts = useAccounts();
  const [adding, setAdding] = useState(false);
  const [busy, setBusy] = useState(false);
  const [removal, setRemoval] = useState<Removal | null>(null);

  const accountName = (id: string | null) => {
    const a = accounts.data?.find((x) => x.id === id);
    return a ? (a.label ?? a.username) : null;
  };

  const run = async (f: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await f();
    } finally {
      setBusy(false);
    }
  };

  const confirmRemoval = async () => {
    const r = removal;
    if (!r) return;
    setRemoval(null);
    const result = await removeSubmodule(repoId, r.info.submodule.path, r.mode, r.force);
    if (result === "dirty") setRemoval({ ...r, force: true });
  };

  if (list.isPending) {
    return (
      <li className="flex items-center gap-1.5 py-1 pr-2 pl-8 text-[11px] text-muted-foreground italic">
        <LoaderCircle className="size-3 animate-spin" />
        {t("submodule.loading")}
      </li>
    );
  }
  if (list.isError) {
    return (
      <li className="py-1 pr-2 pl-8 text-[11px] text-danger">
        {t("submodule.error", { message: errorMessage(list.error) })}
      </li>
    );
  }
  const all = list.data ?? [];
  const shown = all.filter((s) =>
    s.submodule.path.toLowerCase().includes(filter.trim().toLowerCase()),
  );
  const initCount = needsInit(all);

  return (
    <>
      {all.length === 0 && (
        <li className="py-1 pr-2 pl-8 text-[11px] text-muted-foreground italic">
          {t("submodule.none")}
        </li>
      )}
      {shown.map((info) => {
        const s = info.submodule;
        const status = submoduleStatus(s);
        const account = accountName(info.account.accountId);
        return (
          <ContextMenu key={s.path}>
            <ContextMenuTrigger asChild>
              <li
                title={[
                  s.path,
                  s.url ?? "",
                  t("submodule.recorded", { oid: short(s.recorded) }),
                  t("submodule.checkedOut", { oid: short(s.head) }),
                  t(`submodule.account.${accountNoteKey(info.account)}`, {
                    account: account ?? t("submodule.noAccount"),
                  }),
                ]
                  .filter(Boolean)
                  .join("\n")}
                style={{ paddingLeft: (8 + s.depth * 3) * 4 }}
                className="flex h-6 cursor-default items-center gap-2 pr-2 text-xs hover:bg-surface-2 data-[state=open]:bg-surface-2"
                onDoubleClick={() => void openSubmodule(root, s.path)}
              >
                <Layers className="size-3.5 shrink-0 text-muted-foreground" />
                <span className="min-w-0 flex-1 truncate">{baseName(s.path)}</span>
                <span className={cn("shrink-0 text-[10px]", TONE[status.tone])}>
                  {t(`submodule.state.${status.key}`)}
                </span>
                {info.account.hostMismatch && (
                  <CircleAlert
                    className="size-3 shrink-0 text-warning"
                    aria-label={t("submodule.account.mismatch", { account: account ?? "" })}
                  />
                )}
              </li>
            </ContextMenuTrigger>
            <ContextMenuContent className="w-64">
              <ContextMenuLabel className="truncate">{s.path}</ContextMenuLabel>
              <ContextMenuItem
                disabled={s.state === "uninitialized"}
                onSelect={() => void openSubmodule(root, s.path)}
              >
                <FolderOpen />
                {t("submodule.open")}
              </ContextMenuItem>
              <ContextMenuSeparator />
              {s.state === "uninitialized" ? (
                <ContextMenuItem
                  onSelect={() => void run(() => updateSubmodules(repoId, [s.path], true))}
                >
                  <Download />
                  {t("submodule.init")}
                </ContextMenuItem>
              ) : (
                <ContextMenuItem
                  onSelect={() => void run(() => updateSubmodules(repoId, [s.path], false))}
                >
                  <RefreshCw />
                  {t("submodule.updateToRecorded")}
                </ContextMenuItem>
              )}
              <ContextMenuItem onSelect={() => void run(() => syncSubmodules(repoId, [s.path]))}>
                <Link2 />
                {t("submodule.sync")}
              </ContextMenuItem>
              <ContextMenuItem onSelect={() => void navigator.clipboard.writeText(s.url ?? s.path)}>
                <Copy />
                {t("submodule.copyUrl")}
              </ContextMenuItem>
              {s.parent === null && (
                <>
                  <ContextMenuSeparator />
                  <ContextMenuItem
                    disabled={s.state === "uninitialized"}
                    onSelect={() => setRemoval({ info, mode: "deinit", force: false })}
                  >
                    <Unplug />
                    {t("submodule.deinit")}
                  </ContextMenuItem>
                  <ContextMenuItem
                    variant="destructive"
                    onSelect={() => setRemoval({ info, mode: "remove", force: false })}
                  >
                    <Trash2 />
                    {t("submodule.remove")}
                  </ContextMenuItem>
                </>
              )}
            </ContextMenuContent>
          </ContextMenu>
        );
      })}
      <li className="flex gap-1 px-2 pt-1 pl-7">
        {all.length > 0 && (
          <Button
            size="xs"
            variant="ghost"
            className="text-muted-foreground"
            disabled={busy}
            title={t("submodule.updateAllHint")}
            onClick={() => void run(() => updateSubmodules(repoId, [], true))}
          >
            {busy ? <LoaderCircle className="animate-spin" /> : <RefreshCw />}
            {initCount > 0
              ? t("submodule.initAll", { count: initCount })
              : t("submodule.updateAll")}
          </Button>
        )}
        <Button
          size="xs"
          variant="ghost"
          className="text-muted-foreground"
          onClick={() => setAdding(true)}
        >
          <Plus />
          {t("submodule.add")}
        </Button>
      </li>
      <AddSubmoduleDialog
        key={adding ? "open" : "closed"}
        repoId={repoId}
        open={adding}
        onClose={() => setAdding(false)}
      />
      <ConfirmDialog
        open={removal !== null}
        title={t(
          removal?.force
            ? "submodule.forceTitle"
            : removal?.mode === "remove"
              ? "submodule.removeTitle"
              : "submodule.deinitTitle",
          { path: removal?.info.submodule.path ?? "" },
        )}
        description={t(
          removal?.force
            ? "submodule.forceBody"
            : removal?.mode === "remove"
              ? "submodule.removeBody"
              : "submodule.deinitBody",
        )}
        confirmLabel={t(
          removal?.force
            ? "submodule.forceConfirm"
            : removal?.mode === "remove"
              ? "submodule.remove"
              : "submodule.deinit",
        )}
        destructive
        onConfirm={() => void confirmRemoval()}
        onCancel={() => setRemoval(null)}
      />
    </>
  );
}
