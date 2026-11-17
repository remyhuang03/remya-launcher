"""Check packaging contracts without installing or contacting user services."""
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest import mock

from tool.install import (
    APP_ID,
    APPS_SIDECAR,
    APPS_UNIT,
    BUNDLE_BINARIES,
    CALCULATOR_SIDECAR,
    CLIPBOARD_SERVICE,
    PACKAGE,
    UNIT,
    apps_unit,
    clipboard_unit,
    install,
    require_bundle,
    require_core,
)


class PackagingTest(unittest.TestCase):
    def test_core_activation_discovery_uses_absolute_xdg_roots(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            environment = {"HOME": temporary, "XDG_DATA_HOME": str(root / "data"),
                           "XDG_DATA_DIRS": "relative:" + str(root / "system")}
            with self.assertRaises(RuntimeError):
                require_core(environment)
            activation = root / "system/dbus-1/services/top.remya.Core1.service"
            activation.parent.mkdir(parents=True)
            activation.touch()
            require_core(environment)
            environment["XDG_DATA_DIRS"] = "relative"
            with self.assertRaises(RuntimeError):
                require_core(environment)
            activation = root / "data/dbus-1/services/top.remya.Core1.service"
            activation.parent.mkdir(parents=True)
            activation.touch()
            require_core(environment)

    def test_registration_defaults_match_policy_and_unit_keeps_hardening(self):
        root = Path(__file__).resolve().parents[1]
        registration = json.loads((root / "linux/packaging/remya-launcher.json").read_text())
        self.assertEqual(registration["id"], "remya-launcher")
        self.assertEqual(registration["config"], "launcher")
        self.assertEqual(
            [page["id"] for page in registration["pages"]],
            ["clipboard", "emoji", "material-theme"],
        )
        self.assertEqual({key: row["default"] for key, row in registration["settings"].items()}, {
            "clipboard-history": True, "clipboard-max-items": 500, "clipboard-retention-days": 30,
            "emoji-auto-paste": False, "emoji-recents": False,
        })
        unit = clipboard_unit(Path('/tmp/a b/clipboard'))
        for line in ["Wants=top.remya.core.service",
                     "After=graphical-session.target top.remya.core.service", "UMask=0077", "LimitCORE=0"]:
            self.assertIn(line, unit.splitlines())
        apps_svc = apps_unit(Path('/tmp/a b/apps'))
        for line in ['ExecStart="/tmp/a b/apps" --daemon', "KillMode=process",
                     "After=graphical-session.target", "UMask=0077", "LimitCORE=0"]:
            self.assertIn(line, apps_svc.splitlines())
        desktop = (root / "linux/packaging/top.remya.launcher.desktop").read_text()
        self.assertIn("[Desktop Action settings]\nName=Settings\nExec=remya_launcher --settings", desktop)

    def test_bundle_requires_launcher_clipboard_and_calculator(self):
        with tempfile.TemporaryDirectory() as temporary:
            bundle = Path(temporary) / "bundle"
            bundle.mkdir()
            for missing in BUNDLE_BINARIES:
                for binary in BUNDLE_BINARIES:
                    path = bundle / binary
                    if binary == missing:
                        path.unlink(missing_ok=True)
                    else:
                        path.touch()
                with self.assertRaisesRegex(RuntimeError, missing):
                    require_bundle(bundle)
            for binary in BUNDLE_BINARIES:
                (bundle / binary).touch()
            require_bundle(bundle)

    def test_install_deploys_calculator_sidecar_without_daemon_or_extra_symlink(self):
        repo_root = Path(__file__).resolve().parents[1]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "repo"
            bundle = root / "build/linux/x64/release/bundle"
            (bundle / "data/licenses").mkdir(parents=True)
            for binary in BUNDLE_BINARIES:
                (bundle / binary).write_text(binary)
            (bundle / "data/licenses/rust-notices.txt").write_text("notice text")
            (root / "linux/packaging").mkdir(parents=True)
            shutil.copyfile(
                repo_root / "linux/packaging/top.remya.launcher.desktop",
                root / f"linux/packaging/{PACKAGE}.desktop",
            )
            shutil.copyfile(
                repo_root / "linux/packaging/remya-launcher.json",
                root / "linux/packaging/remya-launcher.json",
            )
            (root / "assets/icons").mkdir(parents=True)
            (root / "assets/icons/app_icon.svg").write_text("<svg/>")

            home = Path(temporary) / "home"
            data = home / ".local/share"
            activation = data / "dbus-1/services/top.remya.Core1.service"
            activation.parent.mkdir(parents=True)
            activation.touch()
            environment = {"HOME": str(home), "XDG_DATA_HOME": str(data)}

            ok = subprocess.CompletedProcess(["systemctl"], 0, "", "")
            with (
                mock.patch("tool.install.require_inter_font"),
                mock.patch("tool.install.shutil.which", side_effect=lambda cmd: cmd if cmd == "systemctl" else None),
                mock.patch("tool.install.systemctl", return_value=ok),
            ):
                install(root, environment)

            destination = data / "remya" / APP_ID
            self.assertEqual((destination / APP_ID).read_text(), APP_ID)
            self.assertEqual((destination / CLIPBOARD_SERVICE).read_text(), CLIPBOARD_SERVICE)
            self.assertEqual((destination / CALCULATOR_SIDECAR).read_text(), CALCULATOR_SIDECAR)
            self.assertEqual((destination / APPS_SIDECAR).read_text(), APPS_SIDECAR)
            self.assertEqual((destination / "data/licenses/rust-notices.txt").read_text(), "notice text")

            bin_dir = home / ".local/bin"
            self.assertTrue((bin_dir / APP_ID).is_symlink())
            self.assertEqual((bin_dir / APP_ID).resolve(), (destination / APP_ID).resolve())
            self.assertFalse((bin_dir / CALCULATOR_SIDECAR).exists())
            self.assertFalse((bin_dir / APPS_SIDECAR).exists())
            self.assertFalse((bin_dir / CLIPBOARD_SERVICE).exists())

            units_dir = data / "systemd/user"
            self.assertEqual(sorted(path.name for path in units_dir.iterdir()), sorted([UNIT, APPS_UNIT]))

            # Unrelated binary at ~/.local/bin/remya_launcher must not be overwritten.
            (bin_dir / APP_ID).unlink()
            (bin_dir / APP_ID).write_text("unrelated")
            with (
                mock.patch("tool.install.require_inter_font"),
                mock.patch("tool.install.shutil.which", side_effect=lambda cmd: cmd if cmd == "systemctl" else None),
                mock.patch("tool.install.systemctl", return_value=ok),
                self.assertRaisesRegex(RuntimeError, "Refusing to replace unrelated command"),
            ):
                install(root, environment)
            self.assertEqual((bin_dir / APP_ID).read_text(), "unrelated")

            # Failure while stopping Apps must still restart Clipboard inside rollback.
            (bin_dir / APP_ID).unlink()
            (bin_dir / APP_ID).symlink_to(destination / APP_ID)
            calls = []

            def failing_apps_stop(*arguments, check=True):
                calls.append(arguments)
                if arguments == ("stop", APPS_UNIT):
                    raise subprocess.CalledProcessError(1, ["systemctl", *arguments])
                return ok

            with (
                mock.patch("tool.install.require_inter_font"),
                mock.patch("tool.install.shutil.which", side_effect=lambda cmd: cmd if cmd == "systemctl" else None),
                mock.patch("tool.install.systemctl", side_effect=failing_apps_stop),
                self.assertRaises(subprocess.CalledProcessError),
            ):
                install(root, environment)
            self.assertIn(("start", UNIT), calls)
