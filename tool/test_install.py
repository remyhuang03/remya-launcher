"""Check packaging contracts without installing or contacting user services."""
import json
from pathlib import Path
import tempfile
import unittest

from tool.install import clipboard_unit, require_core


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
        self.assertEqual({key: row["default"] for key, row in registration["settings"].items()}, {
            "clipboard-history": True, "clipboard-max-items": 500, "clipboard-retention-days": 30,
        })
        unit = clipboard_unit(Path('/tmp/a b/clipboard'))
        for line in ["Wants=top.remya.core.service",
                     "After=graphical-session.target top.remya.core.service", "UMask=0077", "LimitCORE=0"]:
            self.assertIn(line, unit.splitlines())
        desktop = (root / "linux/packaging/top.remya.launcher.desktop").read_text()
        self.assertIn("[Desktop Action settings]\nName=Settings\nExec=remya_launcher --settings", desktop)
