import { useEffect, useRef } from "react";
import { Channel } from "@tauri-apps/api/core";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import i18n from "@/i18n";
import { commands, errorMessage, type TerminalEvent } from "@/ipc";
import { useUi } from "@/stores/ui";
import { terminalKeyHandler } from "./keys";
import { useTerminal } from "./store";
import { terminalFont, xtermTheme } from "./theme";

interface Props {
  tabKey: string;
  repoId: number;
  visible: boolean;
}

const dim = (text: string) => `\r\n\x1b[2m${text}\x1b[0m\r\n`;

/**
 * Fits the terminal to its host. A hidden host has no size (fitting would shrink the PTY to
 * nothing), and right after becoming visible xterm may not have measured its font yet, so this
 * retries for a few frames.
 */
function fitToHost(el: HTMLElement, fit: FitAddon, tries = 10) {
  if (el.clientWidth === 0 || el.clientHeight === 0) return;
  if (fit.proposeDimensions()) fit.fit();
  else if (tries > 0) requestAnimationFrame(() => fitToHost(el, fit, tries - 1));
}

/** One xterm.js instance bound to one backend shell session. */
export default function TerminalView({ tabKey, repoId, visible }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const theme = useUi((s) => s.resolvedTheme);
  const themeVersion = useUi((s) => s.themeVersion);

  useEffect(() => {
    const el = host.current;
    if (!el) return;
    const term = new Terminal({
      fontFamily: terminalFont(),
      fontSize: 13,
      lineHeight: 1.15,
      cursorBlink: true,
      scrollback: 5000,
      theme: xtermTheme(useUi.getState().resolvedTheme),
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(el);
    termRef.current = term;
    fitRef.current = fit;

    let disposed = false;
    let session: number | null = null;
    let exited = false;

    const fitNow = () => {
      if (!disposed) fitToHost(el, fit);
    };

    const channel = new Channel<TerminalEvent>();
    channel.onmessage = (event) => {
      if (disposed) return;
      if (event.kind === "output") {
        term.write(event.data);
        return;
      }
      session = null;
      exited = true;
      term.write(
        dim(
          event.code === null
            ? i18n.t("terminal.exitedUnknown")
            : i18n.t("terminal.exited", { code: event.code }),
        ),
      );
    };

    const start = () => {
      fitNow();
      commands
        .terminalOpen(repoId, term.cols, term.rows, channel)
        .then((info) => {
          if (disposed) {
            void commands.terminalClose(info.id).catch(() => {});
            return;
          }
          // A shell that already exited (its exit event can beat this reply) stays closed.
          if (!exited) session = info.id;
          useTerminal.getState().setTitle(tabKey, info.shell);
        })
        .catch((e: unknown) => {
          if (disposed) return;
          exited = true;
          term.write(
            `\x1b[31m${i18n.t("terminal.openFailed", { message: errorMessage(e) })}\x1b[0m\r\n`,
          );
        });
    };

    const input = term.onData((data) => {
      if (session !== null) {
        void commands.terminalWrite(session, data).catch(() => {});
      } else if (exited && data === "\r") {
        exited = false;
        term.reset();
        start();
      }
    });
    const resize = term.onResize(({ cols, rows }) => {
      if (session !== null) void commands.terminalResize(session, cols, rows).catch(() => {});
    });
    term.attachCustomKeyEventHandler(terminalKeyHandler(term));

    const observer = new ResizeObserver(() => fitNow());
    observer.observe(el);
    start();

    return () => {
      disposed = true;
      observer.disconnect();
      input.dispose();
      resize.dispose();
      if (session !== null) void commands.terminalClose(session).catch(() => {});
      term.dispose();
      termRef.current = null;
      fitRef.current = null;
    };
  }, [repoId, tabKey]);

  useEffect(() => {
    const term = termRef.current;
    if (term) term.options.theme = xtermTheme(theme);
  }, [theme, themeVersion]);

  useEffect(() => {
    const el = host.current;
    const fit = fitRef.current;
    if (!visible || !el || !fit) return;
    fitToHost(el, fit);
    termRef.current?.focus();
  }, [visible]);

  return (
    <div
      ref={host}
      data-testid="terminal-view"
      className={visible ? "absolute inset-0 px-2 pt-1" : "hidden"}
    />
  );
}
