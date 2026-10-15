/**
 * Command palette (`Ctrl/Cmd+K`). Public API for other features:
 *
 * - `registerCommands(source, provider)` — contribute commands (see `registry.ts` for an example).
 * - `openCommandPalette({ commandId?, query? })` — open it, optionally inside a command's sub-list
 *   (e.g. the account pill could open `"accounts.switch"`).
 * - `modKey(ctx)` — `Ctrl` / `⌘` for shortcut hints.
 *
 * Feature contributions: add ONE import line below per module that calls `registerCommands`.
 * The built-in "accounts" placeholders (Switch account…, Commit as…, Push as…, Clone as…) are
 * registered with `{ fallback: true }`; registering the source `"accounts"` replaces them.
 */
import "./builtins";
import "@/features/accounts/commands";
import "@/features/ssh/commands";
import "@/features/clone/commands";
import "@/features/file-history/commands";
import "@/features/rebase/commands";
import "@/features/undo/commands";

export { CommandPalette } from "./CommandPalette";
export {
  registerCommands,
  modKey,
  type CommandProvider,
  type PaletteCommand,
  type PaletteContext,
  type PaletteInput,
} from "./registry";
export { closeCommandPalette, openCommandPalette, toggleCommandPalette } from "./store";
