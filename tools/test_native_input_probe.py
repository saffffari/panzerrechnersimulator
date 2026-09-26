"""CPU-only guards/cleanup tests: never opens D-Bus, uinput or a native window."""
import argparse
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import types
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location("native_input_probe", Path(__file__).with_name("native_input_probe.py"))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


class NativeInputGuardTests(unittest.TestCase):
    def test_only_supported_bounded_controls_are_accepted(self):
        accepted = [
            {"op":"events","device":"keyboard","events":[["EV_KEY","KEY_SPACE",1],["EV_KEY","KEY_SPACE",0]]},
            {"op":"events","device":"mouse","events":[["EV_REL","REL_X",1200]]},
            {"op":"events","device":"pad1","events":[["EV_ABS","ABS_HAT2X",255]]},
            {"op":"wait","seconds":0.2},
        ]
        for command in accepted:
            self.assertEqual(probe.validate_command(command),command)
        rejected = [
            {"op":"events","device":"keyboard","events":[["EV_KEY","KEY_LEFTMETA",1]]},
            {"op":"events","device":"mouse","events":[["EV_REL","REL_X",1201]]},
            {"op":"events","device":"pad1","events":[["EV_ABS","ABS_Z",255]]},
            {"op":"events","device":"pad1","events":[["EV_ABS","ABS_HAT2X",256]]},
            {"op":"events","device":"pad1","events":[["EV_KEY","BTN_SOUTH",True]]},
            {"op":"wait","seconds":float("nan")}, {"op":"wait","seconds":11},
            {"op":"create","device":"/dev/input/event0"}, {"op":"shell","command":"anything"},
        ]
        for command in rejected:
            with self.assertRaises(ValueError,msg=str(command)):
                probe.validate_command(command)

    def test_focus_mismatch_fails_closed(self):
        guard=probe.KWinFocusGuard.__new__(probe.KWinFocusGuard)
        guard.identity={"pid":123}
        guard.query=lambda:{"pid":456}
        with self.assertRaisesRegex(RuntimeError,"Focus guard stopped"):
            guard.require_target()
        guard.query=lambda:{"pid":123,"nonce":"fresh"}
        self.assertEqual(guard.require_target()["nonce"],"fresh")

    def test_focus_failure_prevents_device_creation_and_records_cleanup(self):
        created=[]
        class Guard:
            identity={"pid":123}
            def __init__(self,pid): pass
            def require_target(self): raise RuntimeError("focus mismatch")
        class Devices:
            def create(self,name): created.append(name)
            def close(self): return []
        with tempfile.TemporaryDirectory() as temp:
            scenario=Path(temp)/"scenario.json"
            scenario.write_text(json.dumps({"commands":[{"op":"create","device":"keyboard"}]}))
            receipt=Path(temp)/"receipt.json"
            args=argparse.Namespace(scenario=str(scenario),execute=True,check_focus=False,pid=123,
                receipt=str(receipt),interactive=False,max_seconds=1)
            with mock.patch.object(probe,"KWinFocusGuard",Guard),mock.patch.object(probe,"VirtualDevices",Devices),contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(probe.run(args),1)
            self.assertEqual(created,[])
            result=json.loads(receipt.read_text())
            self.assertFalse(result["completed"])
            self.assertEqual(result["cleanup_errors"],[])

    def test_cleanup_releases_every_key_and_centers_every_pad_axis(self):
        devices=probe.VirtualDevices.__new__(probe.VirtualDevices)
        names=set(probe.KEYS+probe.PAD_BUTTONS+list(probe.PAD_AXES)+["EV_KEY","EV_ABS"])
        devices.evdev=types.SimpleNamespace(ecodes=types.SimpleNamespace(**{n:n for n in names}))
        class Device:
            def __init__(self): self.events=[];self.closed=False;self.synced=False
            def write(self,*event): self.events.append(event)
            def syn(self):self.synced=True
            def close(self):self.closed=True
        keyboard,pad=Device(),Device()
        devices.open={"keyboard":keyboard,"pad1":pad}
        self.assertEqual(devices.close(),[])
        self.assertEqual(len(keyboard.events),len(probe.KEYS))
        self.assertTrue(all(e[2]==0 for e in keyboard.events+pad.events))
        self.assertTrue(all(("EV_ABS",axis,0) in pad.events for axis in probe.PAD_AXES))
        self.assertTrue(keyboard.closed and pad.closed and keyboard.synced and pad.synced)
        self.assertFalse(devices.open)

    def test_cleanup_still_closes_a_device_when_release_write_fails(self):
        devices=probe.VirtualDevices.__new__(probe.VirtualDevices)
        devices.evdev=types.SimpleNamespace(ecodes=types.SimpleNamespace(EV_KEY=1,KEY_W=17))
        class Broken:
            closed=False
            def write(self,*args):raise OSError("disconnected")
            def close(self):self.closed=True
        broken=Broken();devices.open={"keyboard":broken}
        self.assertEqual(len(devices.close()),1)
        self.assertTrue(broken.closed)

    def test_dry_run_does_not_construct_guard_or_virtual_devices(self):
        args=argparse.Namespace(scenario=None,execute=False,check_focus=False)
        with mock.patch.object(probe,"KWinFocusGuard",side_effect=AssertionError("must not query KWin")),mock.patch.object(probe,"VirtualDevices",side_effect=AssertionError("must not open uinput")),contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(probe.run(args),0)


if __name__ == "__main__":
    unittest.main()
