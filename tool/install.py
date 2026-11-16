#!/usr/bin/env python3
"""Install a built release bundle in XDG_DATA_HOME without changing configuration."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

APP_ID = "remya_launcher"
PACKAGE = "top.remya.launcher"
CLIPBOARD_SERVICE = "remya-launcher-clipboard"
CALCULATOR_SIDECAR = "remya-launcher-calculator"
APPS_SIDECAR = "remya-launcher-apps"
WEB_SIDECAR = "remya-launcher-web"
BUNDLE_BINARIES = (APP_ID, CLIPBOARD_SERVICE, CALCULATOR_SIDECAR, APPS_SIDECAR, WEB_SIDECAR)
UNIT = f"{PACKAGE}.clipboard.service"
APPS_UNIT = f"{PACKAGE}.apps.service"


def data_home(environment):
    value = environment.get("XDG_DATA_HOME", "")
    if value.startswith("/"):
        return Path(value)
    home = environment.get("HOME", "")
    if not home.startswith("/"):
        raise ValueError("An absolute HOME or XDG_DATA_HOME is required")
    return Path(home) / ".local/share"


def desktop_quote(path):
    value = str(path).replace("%", "%%")
    for character in ('\\', '"', '`', '$'):
        value = value.replace(character, "\\" + character)
    return '"' + value.replace("\\", "\\\\") + '"'


def unit_quote(path):
    value = str(path).replace("\\", "\\\\").replace('"', '\\"')
    return '"' + value.replace("%", "%%").replace("$", "$$") + '"'


def clipboard_unit(executable):
    return (
        "[Unit]\nDescription=Remya Launcher clipboard history\n"
        "PartOf=graphical-session.target\nWants=top.remya.core.service\n"
        "After=graphical-session.target top.remya.core.service\n\n"
        f"[Service]\nExecStart={unit_quote(executable)}\n"
        "Restart=on-failure\nRestartSec=2\nUMask=0077\nLimitCORE=0\n\n"
        "[Install]\nWantedBy=graphical-session.target\n"
    )


def apps_unit(executable):
    return (
        "[Unit]\nDescription=Remya Launcher apps index\n"
        "PartOf=graphical-session.target\n"
        "After=graphical-session.target\n\n"
        f"[Service]\nExecStart={unit_quote(executable)} --daemon\n"
        "KillMode=process\n"
        "Restart=on-failure\nRestartSec=2\nUMask=0077\nLimitCORE=0\n\n"
        "[Install]\nWantedBy=graphical-session.target\n"
    )


def systemctl(*arguments, check=True):
    return subprocess.run(
        ["systemctl", "--user", *arguments], check=check, capture_output=not check
    )


def require_inter_font():
    try:
        result = subprocess.run(['fc-match', '--format=%{family}', 'Inter'], capture_output=True, text=True, check=True)
    except (OSError, subprocess.CalledProcessError) as error:
        raise RuntimeError("Install fontconfig and the Inter system font first (Arch: sudo pacman -S inter-font)") from error
    if 'Inter' not in result.stdout.strip().split(','):
        raise RuntimeError("Install the Inter system font first (Arch: sudo pacman -S inter-font)")


def require_core(environment):
    roots = [data_home(environment)]
    roots.extend(Path(value) for value in (environment.get("XDG_DATA_DIRS") or "/usr/local/share:/usr/share").split(":") if value.startswith("/"))
    if not any((root / "dbus-1/services/top.remya.Core1.service").is_file() for root in roots):
        raise RuntimeError("Install Remya Core first (top.remya.Core1.service is required)")


def require_bundle(source):
    missing = [binary for binary in BUNDLE_BINARIES if not (source / binary).is_file()]
    if missing:
        raise RuntimeError(
            f"Run fvm flutter build linux --release first (missing {', '.join(missing)})"
        )


def install(root, environment):
    require_core(environment)
    require_inter_font()
    data = data_home(environment).resolve()
    source = root / "build/linux/x64/release/bundle"
    require_bundle(source)
    destination = data / "remya" / APP_ID
    home = environment.get("HOME", "")
    if not home.startswith("/"):
        raise ValueError("An absolute HOME is required for ~/.local/bin")
    command = Path(home) / ".local/bin" / APP_ID
    if command.exists() or command.is_symlink():
        if not command.is_symlink() or command.resolve() != destination / APP_ID:
            raise RuntimeError(f"Refusing to replace unrelated command: {command}")
    if not shutil.which("systemctl"):
        raise RuntimeError("The clipboard watcher requires systemd user services")
    if systemctl("show", "--property=Version", "--value", check=False).returncode != 0:
        raise RuntimeError("Cannot connect to the systemd user manager")
    was_active = {
        unit: systemctl("is-active", "--quiet", unit, check=False).returncode == 0
        for unit in (UNIT, APPS_UNIT)
    }
    try:
        for unit, active in was_active.items():
            if active:
                systemctl("stop", unit)
        install_bundle(root, source, destination, data, command)
        units = data / "systemd/user"
        units.mkdir(parents=True, exist_ok=True)
        (units / UNIT).write_text(clipboard_unit(destination / CLIPBOARD_SERVICE))
        (units / APPS_UNIT).write_text(apps_unit(destination / APPS_SIDECAR))
        systemctl("daemon-reload")
        graphical_active = (
            systemctl("is-active", "--quiet", "graphical-session.target", check=False).returncode == 0
        )
        for unit, active in was_active.items():
            systemctl("enable", unit)
            if active or graphical_active:
                systemctl("start", unit)
    except Exception:
        for unit, active in was_active.items():
            if active:
                systemctl("start", unit, check=False)
        raise
    print(f"Installed {PACKAGE} in {destination}")
    print(f"Command: {command} (add ~/.local/bin to PATH if needed)")
    print(f"Autostart: {UNIT} and {APPS_UNIT} start with the graphical session")


def install_bundle(root, source, destination, data, command):
    for process in Path("/proc").iterdir():
        if not process.name.isdecimal():
            continue
        try:
            executable = (process / "exe").resolve()
        except (OSError, RuntimeError):
            continue
        if executable.is_relative_to(destination):
            raise RuntimeError(f"Stop {APP_ID} before updating its installed bundle")
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=f".{APP_ID}-", dir=destination.parent) as temporary:
        staging = Path(temporary) / "bundle"
        shutil.copytree(source, staging, symlinks=True)
        backup = Path(temporary) / "previous"
        if destination.exists():
            destination.rename(backup)
        try:
            staging.rename(destination)
        except OSError:
            if backup.exists():
                backup.rename(destination)
            raise
    applications = data / "applications"
    applications.mkdir(parents=True, exist_ok=True)
    entry = (root / f"linux/packaging/{PACKAGE}.desktop").read_text()
    entry = entry.replace(f"Exec={APP_ID}", f"Exec=/usr/bin/env {desktop_quote(destination / APP_ID)}")
    (applications / f"{PACKAGE}.desktop").write_text(entry)
    registrations = data / "remya/apps"
    registrations.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(root / "linux/packaging/remya-launcher.json",
                    registrations / "remya-launcher.json")
    icons = data / "icons/hicolor/scalable/apps"
    icons.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(root / "assets/icons/app_icon.svg", icons / f"{PACKAGE}.svg")
    if shutil.which("gtk-update-icon-cache"):
        subprocess.run(["gtk-update-icon-cache", "-f", "-t", str(data / "icons/hicolor")], check=True)
    command.parent.mkdir(parents=True, exist_ok=True)
    if not command.is_symlink():
        command.symlink_to(destination / APP_ID)
    if shutil.which("update-desktop-database"):
        subprocess.run(["update-desktop-database", str(applications)], check=True)


if __name__ == "__main__":
    install(Path(__file__).resolve().parents[1], os.environ)
