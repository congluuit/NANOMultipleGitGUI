import type { ReactNode } from "react";
import {
  Cloud,
  ExternalLink,
  FileText,
  FolderOpen,
  HardDrive,
  LoaderCircle,
  RefreshCw,
  TriangleAlert,
} from "lucide-react";
import { Trans, useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { openExternal } from "@/features/accounts/manager/openExternal";
import { errorMessage, type LfsServerInfo } from "@/ipc";
import { formatBytes } from "@/lib/format";
import { queryClient } from "@/lib/queryClient";
import { cn } from "@/lib/utils";
import { fileTotals } from "./lfsModel";
import { useLfsManager } from "./managerStore";
import {
  filesOver,
  largestFiles,
  lfsHostOf,
  percentOf,
  providerQuota,
  smallestFileMax,
  usagePageUrl,
  type LfsHost,
  type ProviderQuota,
} from "./quota";
import { lfsKeys, useLfsFiles, useLfsServerUsage, useLfsStorage } from "./queries";

function Section({
  icon: Icon,
  title,
  children,
}: {
  icon: typeof Cloud;
  title: string;
  children: ReactNode;
}) {
  return (
    <section className="grid gap-1.5">
      <h4 className="flex items-center gap-1.5 font-semibold">
        <Icon className="size-3.5" />
        {title}
      </h4>
      {children}
    </section>
  );
}

/** One "where does it live" row: an icon, a bold place and what is kept there. */
function Place({
  icon: Icon,
  title,
  children,
}: {
  icon: typeof Cloud;
  title: string;
  children: ReactNode;
}) {
  return (
    <li className="flex gap-2 px-2 py-1.5">
      <Icon className="mt-0.5 size-3.5 shrink-0 text-muted-foreground" />
      <div className="grid min-w-0 gap-0.5">
        <span className="font-medium">{title}</span>
        <span className="text-muted-foreground">{children}</span>
      </div>
    </li>
  );
}

function Stat({ label, value, hint }: { label: string; value: string; hint?: string }) {
  return (
    <div className="grid gap-0.5 rounded-md border px-2 py-1.5" title={hint}>
      <span className="text-[11px] text-muted-foreground">{label}</span>
      <span className="text-sm font-semibold tabular-nums">{value}</span>
    </div>
  );
}

function Bar({ percent }: { percent: number }) {
  return (
    <div className="h-1.5 overflow-hidden rounded-full bg-surface-2">
      <div
        className={cn("h-full rounded-full", percent >= 90 ? "bg-danger" : "bg-primary")}
        style={{ width: `${percent}%` }}
      />
    </div>
  );
}

function LinkButton({ url, children }: { url: string; children: ReactNode }) {
  return (
    <Button size="xs" variant="outline" onClick={() => void openExternal(url)} title={url}>
      <ExternalLink />
      {children}
    </Button>
  );
}

/** Overview of the LFS manager: where LFS files live, the server's quota and the sizes. */
export function OverviewTab({ repoId }: { repoId: number }) {
  const { t } = useTranslation();
  const files = useLfsFiles(repoId);
  const storage = useLfsStorage(repoId);
  const server = useLfsServerUsage(repoId);
  const setTab = useLfsManager((s) => s.setTab);

  const all = files.data ?? [];
  const totals = fileTotals(all);
  const info = server.data;
  const host: LfsHost = lfsHostOf(info?.host, info?.kind);
  const quota = providerQuota(host);
  const fileMax = smallestFileMax(quota);
  const tooBig = filesOver(all, fileMax);
  const mediaDir = storage.data?.mediaDir ?? ".git/lfs/objects";

  return (
    <div className="grid gap-4 text-xs">
      <Section icon={FolderOpen} title={t("lfsOverview.whereTitle")}>
        <p className="text-muted-foreground">{t("lfsOverview.whereIntro")}</p>
        <ul className="divide-y rounded-md border">
          <Place icon={FolderOpen} title={t("lfsOverview.place.folder")}>
            {t("lfsOverview.place.folderText")}
          </Place>
          <Place icon={FileText} title={t("lfsOverview.place.history")}>
            {t("lfsOverview.place.historyText")}
          </Place>
          <Place icon={HardDrive} title={t("lfsOverview.place.local")}>
            <Trans
              i18nKey="lfsOverview.place.localText"
              values={{ dir: mediaDir }}
              components={{ code: <code className="font-mono text-[11px] break-all" /> }}
            />
          </Place>
          <Place icon={Cloud} title={t("lfsOverview.place.server")}>
            {t("lfsOverview.place.serverText")}
          </Place>
        </ul>
        <ul className="list-disc space-y-0.5 pl-5 text-muted-foreground">
          <li>{t("lfsOverview.flow.push")}</li>
          <li>{t("lfsOverview.flow.pull")}</li>
          <li>{t("lfsOverview.flow.clone")}</li>
        </ul>
        <p className="rounded-md border border-info/40 bg-info/10 px-2 py-1.5 text-info">
          {t("lfsOverview.example")}
        </p>
      </Section>

      <Section icon={HardDrive} title={t("lfsOverview.sizesTitle")}>
        {files.isError && (
          <p className="text-danger">
            {t("lfsFiles.error", { message: errorMessage(files.error) })}
          </p>
        )}
        <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
          <Stat
            label={t("lfsOverview.stat.files")}
            value={files.data ? String(totals.count) : "…"}
          />
          <Stat
            label={t("lfsOverview.stat.head")}
            value={files.data ? formatBytes(totals.bytes) : "…"}
            hint={t("lfsOverview.stat.headHint")}
          />
          <Stat
            label={t("lfsOverview.stat.local")}
            value={storage.data ? formatBytes(storage.data.bytes) : "…"}
            hint={t("lfsOverview.stat.localHint", { count: storage.data?.objects ?? 0 })}
          />
          <Stat
            label={t("lfsOverview.stat.pointers")}
            value={files.data ? String(totals.pointers) : "…"}
            hint={t("lfsOverview.stat.pointersHint")}
          />
        </div>
        <p className="text-muted-foreground">{t("lfsOverview.sizesHint")}</p>
        {all.length > 0 && (
          <>
            <p className="font-medium">{t("lfsOverview.largest")}</p>
            <ul className="divide-y rounded-md border">
              {largestFiles(all).map((f) => (
                <li key={f.path} className="flex h-7 items-center gap-2 px-2">
                  <span className="min-w-0 flex-1 truncate font-mono" title={f.path}>
                    {f.path}
                  </span>
                  <span className="shrink-0 text-muted-foreground tabular-nums">
                    {formatBytes(f.size)}
                  </span>
                </li>
              ))}
            </ul>
          </>
        )}
        {tooBig.length > 0 && fileMax && (
          <p className="flex items-start gap-1.5 rounded-md border border-warning/40 bg-warning/10 px-2 py-1.5 text-warning">
            <TriangleAlert className="mt-0.5 size-3.5 shrink-0" />
            {t("lfsOverview.tooBig", {
              count: tooBig.length,
              limit: fileMax.label,
              provider: quota.name,
            })}
          </p>
        )}
      </Section>

      <Section icon={Cloud} title={t("lfsOverview.serverTitle")}>
        {server.isPending && (
          <p className="flex items-center gap-1 text-muted-foreground">
            <LoaderCircle className="size-3 animate-spin" />
            {t("lfsOverview.serverLoading")}
          </p>
        )}
        {server.isError && <p className="text-danger">{errorMessage(server.error)}</p>}
        {info && <ServerQuota info={info} host={host} quota={quota} />}
        <div>
          <Button
            size="xs"
            variant="ghost"
            onClick={() => void queryClient.invalidateQueries({ queryKey: lfsKeys.server(repoId) })}
          >
            {server.isFetching ? <LoaderCircle className="animate-spin" /> : <RefreshCw />}
            {t("lfsOverview.recheck")}
          </Button>
        </div>
      </Section>

      <Section icon={HardDrive} title={t("lfsOverview.cleanTitle")}>
        <p className="text-muted-foreground">{t("lfsOverview.cleanLocal")}</p>
        <p className="text-muted-foreground">{t(`lfsOverview.cleanServer.${cleanKey(host)}`)}</p>
        <div>
          <Button size="xs" variant="outline" onClick={() => setTab("maintenance")}>
            {t("lfsOverview.openMaintenance")}
          </Button>
        </div>
      </Section>
    </div>
  );
}

function cleanKey(host: LfsHost): "github" | "gitlab" | "other" {
  if (host === "github" || host === "githubEnterprise") return "github";
  if (host === "gitlab" || host === "gitlabSelfHosted") return "gitlab";
  return "other";
}

function ServerQuota({
  info,
  host,
  quota,
}: {
  info: LfsServerInfo;
  host: LfsHost;
  quota: ProviderQuota;
}) {
  const { t } = useTranslation();
  if (!info.remote) return <p className="text-muted-foreground">{t("lfsOverview.noRemote")}</p>;
  const usage = info.usage;
  const organization =
    usage.status === "done" && usage.value.source === "githubBilling"
      ? usage.value.organization
      : null;
  const usageUrl = usagePageUrl(host, info.host, info.fullName, organization);

  return (
    <>
      <p>
        {t("lfsOverview.remote", {
          remote: info.remote,
          project: `${info.host ?? ""}/${info.fullName ?? ""}`,
          provider: quota.name || t("lfsOverview.unknownProvider"),
        })}
      </p>
      <p className="text-muted-foreground">{t(`lfsOverview.host.${host}`)}</p>
      {quota.plans.length > 0 && (
        <table className="w-full border-collapse overflow-hidden rounded-md border text-left">
          <thead className="bg-surface-2 text-[11px] text-muted-foreground">
            <tr>
              <th className="px-2 py-1 font-medium">{t("lfsOverview.col.plan")}</th>
              <th className="px-2 py-1 font-medium">{t("lfsOverview.col.storage")}</th>
              <th className="px-2 py-1 font-medium">{t("lfsOverview.col.bandwidth")}</th>
              <th className="px-2 py-1 font-medium">{t("lfsOverview.col.fileMax")}</th>
            </tr>
          </thead>
          <tbody>
            {quota.plans.map((p) => (
              <tr key={p.plan} className="border-t">
                <td className="px-2 py-1">{p.plan}</td>
                <td className="px-2 py-1 tabular-nums">
                  {p.storage?.label ?? t("lfsOverview.notStated")}
                </td>
                <td className="px-2 py-1 tabular-nums">
                  {p.bandwidth
                    ? t("lfsOverview.perMonth", { size: p.bandwidth.label })
                    : t("lfsOverview.notStated")}
                </td>
                <td className="px-2 py-1 tabular-nums">
                  {p.fileMax?.label ?? t("lfsOverview.noLimit")}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <p className="text-muted-foreground">{t(`lfsOverview.full.${host}`)}</p>
      <p className="text-[11px] text-muted-foreground">
        {t("lfsOverview.checkedOn", { date: "2026-11-23" })}
      </p>

      <p className="font-medium">{t("lfsOverview.usageTitle")}</p>
      <UsageView info={info} host={host} quota={quota} />

      <div className="flex flex-wrap gap-2">
        {usageUrl && <LinkButton url={usageUrl}>{t("lfsOverview.openUsage")}</LinkButton>}
        <LinkButton url={quota.docsUrl}>{t("lfsOverview.openDocs")}</LinkButton>
      </div>
    </>
  );
}

function UsageView({
  info,
  host,
  quota,
}: {
  info: LfsServerInfo;
  host: LfsHost;
  quota: ProviderQuota;
}) {
  const { t } = useTranslation();
  const usage = info.usage;
  if (usage.status === "unavailable") {
    return (
      <p
        className="rounded-md border px-2 py-1.5 text-muted-foreground"
        title={usage.message ?? undefined}
      >
        {t(`lfsOverview.unavailable.${usage.reason}`, { provider: quota.name || info.host })}
      </p>
    );
  }
  const v = usage.value;
  if (v.source === "gitlabProject") {
    const used = v.lfsBytes + v.repositoryBytes;
    const free = host === "gitlab" ? quota.plans[0]?.storage : undefined;
    const pct = free ? percentOf(used, free.bytes) : 0;
    return (
      <div className="grid gap-1 rounded-md border px-2 py-1.5">
        <p>
          {t("lfsOverview.gitlabUsage", {
            lfs: formatBytes(v.lfsBytes),
            repo: formatBytes(v.repositoryBytes),
            total: formatBytes(v.storageBytes),
          })}
        </p>
        {free && (
          <>
            <Bar percent={pct} />
            <p className="text-[11px] text-muted-foreground">
              {t("lfsOverview.gitlabShare", {
                used: formatBytes(used),
                limit: free.label,
                percent: pct,
              })}
            </p>
          </>
        )}
      </div>
    );
  }
  if (v.items.length === 0) {
    return (
      <p className="rounded-md border px-2 py-1.5 text-muted-foreground">
        {t("lfsOverview.githubNone", { owner: v.owner })}
      </p>
    );
  }
  return (
    <ul className="divide-y rounded-md border">
      {v.items.map((i) => (
        <li key={`${i.product}:${i.sku}`} className="grid gap-0.5 px-2 py-1.5">
          <span className="font-medium">{i.sku || i.product}</span>
          <span className="text-muted-foreground">
            {t("lfsOverview.githubItem", {
              quantity: round(i.quantity),
              included: round(i.included),
              unit: i.unitType,
              amount: (i.netAmount ?? 0).toFixed(2),
            })}
          </span>
        </li>
      ))}
    </ul>
  );
}

/** Two decimals at most; Rust sends a non-finite float as `null`. */
const round = (n: number | null) => String(Math.round((n ?? 0) * 100) / 100);
