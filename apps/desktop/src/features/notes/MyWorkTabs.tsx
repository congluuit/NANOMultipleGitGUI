// Phase 6.5: tabs of the "My work" panel: PR / MR & issues | My notes.
import { Inbox, NotebookPen } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import { useNotes } from "./queries";
import { useNotesUi, type MyWorkTab } from "./store";

export function MyWorkTabs() {
  const { t } = useTranslation();
  const tab = useNotesUi((s) => s.tab);
  const setTab = useNotesUi((s) => s.setTab);
  const open = (useNotes().data?.notes ?? []).filter((n) => !n.done).length;
  const tabs: { id: MyWorkTab; label: string; icon: typeof Inbox }[] = [
    { id: "work", label: t("notes.tabWork"), icon: Inbox },
    {
      id: "notes",
      label: open ? t("notes.tabNotesCount", { count: open }) : t("notes.tabNotes"),
      icon: NotebookPen,
    },
  ];
  return (
    <div role="tablist" aria-label={t("mywork.title")} className="flex gap-1 border-b text-xs">
      {tabs.map(({ id, label, icon: Icon }) => (
        <button
          key={`tab-${id}`}
          type="button"
          role="tab"
          aria-selected={tab === id}
          onClick={() => setTab(id)}
          className={cn(
            "-mb-px inline-flex items-center gap-1.5 border-b-2 border-transparent px-3 py-1.5 text-muted-foreground hover:text-foreground",
            tab === id && "border-primary font-medium text-foreground",
          )}
        >
          <Icon className="size-3.5" />
          {label}
        </button>
      ))}
    </div>
  );
}
