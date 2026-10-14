# 0001. Tauri 2 + Rust + React/TypeScript as the desktop stack

- Status: Accepted
- Date: 2026-10-14

## Context

NANOMultipleGitGUI must ship as a Windows `.exe` installer and a macOS `.dmg` (Linux as a bonus), render large commit graphs and diffs smoothly, store secrets in the OS keychain and run git/git-lfs as child processes with per-account environments. It is a volunteer-driven open-source project, so contributor accessibility and small downloads matter, and there is no budget for an Apple Developer ID.

Options considered: Electron + Node, Tauri 2 + Rust, native per platform (WinUI + SwiftUI), Qt/C++, Flutter.

## Decision

Use **Tauri 2** with a **Rust** backend (Cargo workspace of focused crates) and a **React + TypeScript + Vite** frontend. IPC types are generated from Rust with tauri-specta so the frontend calls typed `commands.*` functions.

## Consequences

- Installers are small (the unsigned NSIS installer of the Phase 0 app is 1.3 MB vs ~150 MB for Electron) and memory use is low because the OS webview is reused (WebView2 on Windows, WKWebView on macOS).
- Rust gives memory safety and good crates for keychain, local sockets, SSH keys and HTTP; heavy work (log parsing, graph layout) runs natively.
- The web UI ecosystem (Radix/shadcn, Monaco, xterm.js) stays available and approachable for contributors.
- Webview differences between platforms must be tested (WebKit on macOS/Linux vs Chromium on Windows); CI runs on all three.
- tauri-specta is pre-1.0 and must be pinned exactly (`=2.0.0-rc.25`); upgrades are deliberate.
- On Windows, test binaries that link the dialog plugin need the Common-Controls v6 manifest; `build.rs` embeds it for every target.
- Tauri's bundler covers NSIS/MSI/DMG/AppImage/deb/rpm, ad-hoc macOS signing and a `signCommand` hook for Windows signing, which fits the "no paid certificates" constraint (see PLAN §8).

## Evidence

Phase 0 builds: `.exe` 1.3 MB, `.msi` 2.0 MB; the release app starts and answers IPC on Windows 10; a 100k-commit graph renders at the display refresh rate (ADR 0002).
