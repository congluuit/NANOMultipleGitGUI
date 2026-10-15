import { LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useAccountLabelOf } from "@/features/ci/hooks";
import { describeForkFailure } from "./model";
import { useForkNamespaces } from "./queries";

const SELF = "__self__";

/**
 * Fork destination of `accountId`: the account itself (`null`) or one of its organizations /
 * groups. Errors (missing scope…) are shown under the select; the account itself still works.
 */
export function NamespaceSelect({
  accountId,
  value,
  onChange,
  disabled,
}: {
  accountId: string;
  /** `null` = the account's own namespace. */
  value: string | null;
  onChange: (namespace: string | null) => void;
  disabled?: boolean;
}) {
  const { t, i18n } = useTranslation();
  const labelOf = useAccountLabelOf();
  const query = useForkNamespaces(accountId);
  const data = query.data;
  const list = data?.status === "done" ? data.value : [];
  const self = list.find((n) => n.kind === "user");
  const failure =
    data && data.status !== "done"
      ? describeForkFailure(t, data, accountId, labelOf(accountId), i18n.language)
      : null;
  return (
    <div className="space-y-1">
      <div className="flex items-center gap-2">
        <Select
          value={value ?? SELF}
          onValueChange={(v) => onChange(v === SELF ? null : v)}
          disabled={disabled}
        >
          <SelectTrigger className="w-64 text-xs" aria-label={t("forks.namespace")}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={SELF} className="text-xs">
              {self ? t("forks.namespaceSelf", { name: self.path }) : t("forks.namespaceSelfPlain")}
            </SelectItem>
            {list
              .filter((n) => n.kind !== "user")
              .map((n) => (
                <SelectItem key={n.path} value={n.path} className="text-xs">
                  {n.path}
                  {n.name && n.name !== n.path && (
                    <span className="ml-1 text-muted-foreground">· {n.name}</span>
                  )}
                </SelectItem>
              ))}
          </SelectContent>
        </Select>
        {query.isFetching && <LoaderCircle className="size-3 animate-spin text-muted-foreground" />}
      </div>
      {failure && (
        <p className="text-[11px] text-warning" role="note">
          {failure.text}
        </p>
      )}
    </div>
  );
}
