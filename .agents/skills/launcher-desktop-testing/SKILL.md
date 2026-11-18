---
name: launcher-desktop-testing
description: Debug and smoke-test the Remya Launcher Linux bundle in a live Hyprland session. Use for launcher input, IME, overlay lifecycle, native drag and drop, sidecars, and installed settings verification.
---

# Test Launcher on Hyprland

Read [AGENTS.md](../../../AGENTS.md) for build, install, and delivery checks. Run the live smoke in the user's graphical session so it inherits the Wayland socket, Hyprland instance, and session bus. After a runtime change, build debug then release, install with `python tool/install.py`, and verify the installed binary. Compare the resolved executable and relevant sidecar with the build using `readlink -f` and `sha256sum` if behavior looks stale.

```bash
python .agents/skills/launcher-desktop-testing/scripts/check-installed.py
```

The script checks live Apps IPC, overlay mapping, and close via `--toggle`. It requires `hyprctl`, `busctl`, and a free Launcher D-Bus name. Wait for the existing Launcher to close before running it. Pass the installed executable as the optional argument if it is outside `~/.local/bin`.

## Diagnose input and focus

- Launch without flags for real search, or use `--settings files-history` to check the installed preference page. `--preview=files` uses static example data. Live Launcher is a single GTK application instance, so a second invocation can focus or close the existing overlay. Match observed layers and clients to the PID you started using `hyprctl -j layers` and `hyprctl -j clients`.
- Check key events and committed text separately. Synthetic text can go through a GTK input method and may leave preedit active, delaying prefix transitions, search updates, or Tab completion. If an ASCII automation probe fails, repeat it in a fresh process with `GTK_IM_MODULE=gtk-im-context-simple remya_launcher`. This is a test-only override. Verify CJK composition, committed text, and shortcut behavior separately with the user's real IME. Leave the session environment and IME settings alone.
- Check the output and live layer geometry before pointer automation. Monitor focus, scale, and layout can change between runs. Get the bounds from the relevant layer (and the output position from `hyprctl -j monitors` when applying an offset). Avoid reusing coordinates across output changes.
- To verify Files drag and drop, drag a disposable file into a Wayland receiver accepting `text/uri-list` and assert the received URI. If the drag starts but the receiver never sees it, use `WAYLAND_DEBUG=1` on the test process. Check that both launcher and dismiss surfaces commit their empty input regions during the drag. The GTK runner queues a redraw so Wayland commits the input-region change.

## Separate packaging, service, and UI failures

- Apps and Clipboard run under the user units `top.remya.launcher.apps.service` and `top.remya.launcher.clipboard.service`. Check `systemctl --user status` and `journalctl --user -u <unit> -b`. Files, Calculator, and Web start on demand from the bundle. For IPC problems, use the matching Rust crate's integration tests to isolate the protocol, and keep stderr separate from NDJSON stdout.
- To investigate a theme flash, compare the earliest visible launcher frame with a settled frame. A nested Hyprland session and a private D-Bus instance let you vary the theme response and inject delays without replacing the live Core service.
- Core may briefly return an unknown preference immediately after installation while it reloads the app registry. Retry the exact read before diagnosing a schema or registration mismatch. Use temporary files and isolated XDG state for history and destructive action probes.
- Close only verification windows, remove temporary fixtures and logs, and report which checks ran against the installed build. Keep untested IME, hardware, and cross-app interactions explicit.
