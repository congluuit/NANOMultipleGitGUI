/** Palette commands of step 3B.5: show / hide the side panels, reset their widths. */
import { Columns3, PanelLeft, PanelRight } from "lucide-react";
import {
  modKey,
  registerCommands,
  type PaletteCommand,
  type PaletteContext,
} from "@/features/command-palette/registry";
import { useLayout } from "@/stores/layout";

export function layoutCommands(c: PaletteContext): PaletteCommand[] {
  const { t } = c;
  const group = t("cmd.group.app");
  return [
    {
      id: "layout.toggleSidebar",
      title: t("layout.cmd.toggleSidebar"),
      group,
      icon: PanelLeft,
      shortcut: `${modKey(c)}+B`,
      keywords: ["sidebar", "panel", "thanh ben", "an hien"],
      run: () => useLayout.getState().togglePanel("sidebar"),
    },
    {
      id: "layout.toggleRight",
      title: t("layout.cmd.toggleRight"),
      group,
      icon: PanelRight,
      shortcut: `${modKey(c)}+Alt+B`,
      keywords: ["panel", "commit", "details", "bang phai", "an hien"],
      run: () => useLayout.getState().togglePanel("right"),
    },
    {
      id: "layout.reset",
      title: t("layout.cmd.reset"),
      group,
      icon: Columns3,
      keywords: ["layout", "reset", "width", "bo cuc", "mac dinh"],
      run: () => {
        const s = useLayout.getState();
        s.resetSize("sidebar");
        s.resetSize("right");
        s.setCollapsed("sidebar", false);
        s.setCollapsed("right", false);
      },
    },
  ];
}

registerCommands("core.layout", layoutCommands);
