/** Command palette: "Publish repository…" (no remote yet) and "Add remote…". */
import { CloudUpload, Link } from "lucide-react";
import { registerCommands, type PaletteContext } from "@/features/command-palette/registry";
import { openPublishDialog } from "./store";

export function publishCommands(c: PaletteContext) {
  if (!c.repo) return [];
  const repoId = c.repo.id;
  const noRemote = !!c.refs && c.refs.remotes.length === 0;
  return [
    ...(noRemote
      ? [
          {
            id: "publish.open",
            title: c.t("publish.paletteTitle"),
            group: c.t("cmd.group.repo"),
            icon: CloudUpload,
            keywords: ["publish repository", "create repository", "github", "gitlab", "dang len"],
            run: () => openPublishDialog(repoId),
          },
        ]
      : []),
    {
      id: "publish.addRemote",
      title: c.t("publish.paletteAddRemote"),
      group: c.t("cmd.group.repo"),
      icon: Link,
      keywords: ["add remote", "remote url", "origin", "them remote"],
      run: () => openPublishDialog(repoId, "url"),
    },
  ];
}

registerCommands("publish", publishCommands);
