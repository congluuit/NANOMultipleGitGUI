import { useState, type ReactNode } from "react";
import {
  FileType,
  Files,
  HardDrive,
  LoaderCircle,
  Lock,
  Settings2,
  TriangleAlert,
  Wrench,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { errorMessage } from "@/ipc";
import { formatBytes } from "@/lib/format";
import { cn } from "@/lib/utils";
import { installLfsHooks } from "./actions";
import { fileTotals } from "./lfsModel";
import { useLfsManager, type LfsTab } from "./managerStore";
import { lfsVersion, shouldOfferInstall } from "./patterns";
import {
  useLfsChangeEvents,
  useLfsFiles,
  useLfsLocks,
  useLfsStatus,
  useLfsStorage,
} from "./queries";

function Row({
  children,
  className,
  title,
}: {
  children: ReactNode;
  className?: string;
  title?: string;
}) {
  return (
    <li
      title={title}
      className={cn("flex min-h-6 items-center gap-2 py-0.5 pr-2 pl-8 text-xs", className)}
    >
      {children}
    </li>
  );
}

/** A summary row that opens the LFS manager on a tab. */
function LinkRow({
  icon: Icon,
  label,
  value,
  onClick,
  tone,
}: {
  icon: typeof FileType;
  label: string;
  value: string;
  onClick: () => void;
  tone?: "warning";
}) {
  return (
    <li>
      <button
        type="button"
        onClick={onClick}
        className="flex h-6 w-full items-center gap-2 pr-2 pl-8 text-left text-xs hover:bg-surface-2"
      >
        <Icon className="size-3.5 shrink-0 text-muted-foreground" />
        <span className="flex-1 truncate">{label}</span>
        <span
          className={cn(
            "shrink-0 text-[11px]",
            tone === "warning" ? "text-warning" : "text-muted-foreground",
          )}
        >
          {value}
        </span>
      </button>
    </li>
  );
}

/** Content of the sidebar "LFS" section: status + summary rows opening the LFS manager. */
export function LfsPanel({ repoId }: { repoId: number }) {
  const { t } = useTranslation();
  const status = useLfsStatus(repoId);
  useLfsChangeEvents(repoId);
  const uses = status.data?.usesLfs === true;
  const files = useLfsFiles(repoId, uses);
  const locks = useLfsLocks(repoId, uses);
  const storage = useLfsStorage(repoId, uses);
  const openManager = useLfsManager((s) => s.open);
  const [installing, setInstalling] = useState(false);
  const s = status.data;

  if (status.isPending) {
    return (
      <Row className="text-muted-foreground italic">
        {status.fetchStatus === "fetching" && <LoaderCircle className="size-3 animate-spin" />}
        {t("lfs.loading")}
      </Row>
    );
  }
  if (status.isError || !s) {
    return (
      <Row className="text-danger">{t("lfs.error", { message: errorMessage(status.error) })}</Row>
    );
  }

  const version = lfsVersion(s);
  const offerInstall = shouldOfferInstall(s);
  const open = (tab: LfsTab) => () => openManager(repoId, tab);
  const totals = fileTotals(files.data ?? []);
  const lockList = locks.data?.status === "done" ? locks.data.value.locks : null;
  const mine = lockList?.filter((l) => l.ours).length ?? 0;

  const install = async () => {
    setInstalling(true);
    await installLfsHooks(repoId);
    setInstalling(false);
  };

  return (
    <>
      <Row
        className={version ? "text-muted-foreground" : "text-warning"}
        title={s.gitLfs.raw ?? undefined}
      >
        <span className="shrink-0 whitespace-nowrap">
          {version ? t("lfs.version", { version }) : t("lfs.notInstalled")}
        </span>
        <span aria-hidden>·</span>
        <span className="truncate">{s.usesLfs ? t("lfs.usesLfs") : t("lfs.notUsed")}</span>
      </Row>
      {!version && s.usesLfs && (
        <Row className="text-warning">
          <TriangleAlert className="size-3.5 shrink-0" />
          <span>{t("lfs.missingTool")}</span>
        </Row>
      )}
      {offerInstall && (
        <li className="mx-2 my-1 space-y-1.5 rounded-md border border-warning/40 bg-warning/10 p-2 text-xs text-warning">
          <div className="flex items-start gap-1.5">
            <TriangleAlert className="mt-0.5 size-3.5 shrink-0" />
            <span>{t("lfs.needsInstall")}</span>
          </div>
          <Button
            size="xs"
            variant="outline"
            className="w-full"
            disabled={installing}
            title={t("lfs.installHint")}
            onClick={() => void install()}
          >
            {installing && <LoaderCircle className="animate-spin" />}
            {t("lfs.install")}
          </Button>
        </li>
      )}
      <LinkRow
        icon={FileType}
        label={t("lfs.tab.patterns")}
        value={String(s.patterns.length)}
        onClick={open("patterns")}
      />
      {uses && (
        <>
          <LinkRow
            icon={Files}
            label={t("lfs.tab.files")}
            value={
              files.data
                ? totals.pointers
                  ? t("lfs.filesPointers", { count: totals.count, pointers: totals.pointers })
                  : String(totals.count)
                : "…"
            }
            tone={totals.pointers ? "warning" : undefined}
            onClick={open("files")}
          />
          <LinkRow
            icon={Lock}
            label={t("lfs.tab.locks")}
            value={
              lockList
                ? mine
                  ? t("lfs.locksMine", { count: lockList.length, mine })
                  : String(lockList.length)
                : locks.data?.status === "failed"
                  ? t("lfs.locksUnavailable")
                  : "…"
            }
            onClick={open("locks")}
          />
          <LinkRow
            icon={HardDrive}
            label={t("lfs.tab.maintenance")}
            value={storage.data ? formatBytes(storage.data.bytes) : "…"}
            onClick={open("maintenance")}
          />
        </>
      )}
      <li className="px-2 pt-1">
        <Button
          size="xs"
          variant="ghost"
          className="w-full justify-start pl-6 text-muted-foreground"
          disabled={!version}
          onClick={open(uses ? "files" : "patterns")}
        >
          {uses ? <Settings2 /> : <Wrench />}
          {t("lfs.openManager")}
        </Button>
      </li>
    </>
  );
}
