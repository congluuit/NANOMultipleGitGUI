import { useState } from "react";
import { GitCompareArrows } from "lucide-react";
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
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useRepoRefs } from "@/features/repos/queries";
import type { RepoRefs } from "@/ipc";

/** Another version of the file to compare with: a branch, tag or commit. */
export interface CompareTarget {
  label: string;
  oid: string;
}

const HEX = /^[0-9a-f]{4,64}$/i;

/** Every ref name the user may type, with its commit. */
function refTargets(refs: RepoRefs | undefined): Map<string, string> {
  const map = new Map<string, string>();
  if (!refs) return map;
  if (refs.head.oid) map.set("HEAD", refs.head.oid);
  for (const b of refs.local) map.set(b.name, b.oid);
  for (const r of refs.remotes) for (const b of r.branches) map.set(`${r.name}/${b.name}`, b.oid);
  for (const tag of refs.tags) map.set(tag.name, tag.oid);
  return map;
}

/** Toolbar menu of the editor: compare the file with its default base or another version. */
export function ComparePicker({
  repoId,
  defaultLabel,
  value,
  onChange,
}: {
  repoId: number;
  /** Name of the default comparison ("the staged copy", "HEAD"…). */
  defaultLabel: string;
  value: CompareTarget | null;
  onChange: (target: CompareTarget | null) => void;
}) {
  const { t } = useTranslation();
  const refs = useRepoRefs(repoId).data;
  const [asking, setAsking] = useState(false);
  const label = value?.label ?? defaultLabel;
  const remoteBranches = (refs?.remotes ?? []).flatMap((r) =>
    r.branches.map((b) => ({ name: `${r.name}/${b.name}`, oid: b.oid })),
  );

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            size="xs"
            variant={value ? "secondary" : "ghost"}
            title={t("editor.compareHint")}
            className="max-w-56"
          >
            <GitCompareArrows />
            <span className="truncate">{t("editor.compareWith", { name: label })}</span>
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="max-h-96 w-64">
          <DropdownMenuItem onSelect={() => onChange(null)}>
            {t("editor.compareDefault", {
              name: defaultLabel.charAt(0).toUpperCase() + defaultLabel.slice(1),
            })}
          </DropdownMenuItem>
          {refs?.head.oid && (
            <DropdownMenuItem onSelect={() => onChange({ label: "HEAD", oid: refs.head.oid! })}>
              HEAD
            </DropdownMenuItem>
          )}
          <DropdownMenuSeparator />
          <RefSub
            title={t("editor.compareLocal")}
            items={refs?.local ?? []}
            onPick={(name, oid) => onChange({ label: name, oid })}
          />
          <RefSub
            title={t("editor.compareRemote")}
            items={remoteBranches}
            onPick={(name, oid) => onChange({ label: name, oid })}
          />
          <RefSub
            title={t("editor.compareTags")}
            items={refs?.tags ?? []}
            onPick={(name, oid) => onChange({ label: name, oid })}
          />
          <DropdownMenuSeparator />
          <DropdownMenuItem onSelect={() => setAsking(true)}>
            {t("editor.compareOther")}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      {asking && (
        <OtherDialog
          refs={refs}
          onPick={(target) => {
            setAsking(false);
            onChange(target);
          }}
          onClose={() => setAsking(false)}
        />
      )}
    </>
  );
}

function RefSub({
  title,
  items,
  onPick,
}: {
  title: string;
  items: { name: string; oid: string }[];
  onPick: (name: string, oid: string) => void;
}) {
  const { t } = useTranslation();
  return (
    <DropdownMenuSub>
      <DropdownMenuSubTrigger disabled={items.length === 0}>
        {title} ({items.length})
      </DropdownMenuSubTrigger>
      <DropdownMenuSubContent className="max-h-80 overflow-y-auto">
        {items.length === 0 && <DropdownMenuLabel>{t("editor.compareNone")}</DropdownMenuLabel>}
        {items.map((r) => (
          <DropdownMenuItem key={r.name} onSelect={() => onPick(r.name, r.oid)}>
            <span className="truncate">{r.name}</span>
          </DropdownMenuItem>
        ))}
      </DropdownMenuSubContent>
    </DropdownMenuSub>
  );
}

function OtherDialog({
  refs,
  onPick,
  onClose,
}: {
  refs: RepoRefs | undefined;
  onPick: (target: CompareTarget) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [value, setValue] = useState("");
  const typed = value.trim();
  const known = refTargets(refs).get(typed);
  const oid = known ?? (HEX.test(typed) ? typed : null);
  const submit = () => {
    if (oid) onPick({ label: known ? typed : typed.slice(0, 10), oid });
  };
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{t("editor.compareOtherTitle")}</DialogTitle>
          <DialogDescription>{t("editor.compareOtherHint")}</DialogDescription>
        </DialogHeader>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            submit();
          }}
        >
          <input
            autoFocus
            value={value}
            onChange={(e) => setValue(e.target.value)}
            aria-invalid={typed.length > 0 && !oid}
            aria-label={t("editor.compareOtherTitle")}
            placeholder="a1b2c3d / main / v1.2.0"
            className="h-8 w-full rounded-md border bg-background px-2 font-mono text-xs focus:border-primary focus:outline-none aria-invalid:border-danger"
          />
        </form>
        <DialogFooter>
          <Button variant="outline" size="sm" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button size="sm" disabled={!oid} onClick={submit}>
            {t("editor.compareApply")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
