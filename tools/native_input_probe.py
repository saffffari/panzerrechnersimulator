#!/usr/bin/env python3
"""Bounded, focus-guarded native input for an explicitly selected game process.

Default is validation only. --execute creates devices and injects events; --check-focus only
queries KWin. JSONL stdin keeps devices alive across commands until close/timeout. Never grabs
physical devices, changes permissions, starts a daemon, or selects/focuses a window.
"""
from __future__ import annotations

import argparse
import json
import math
import os
from pathlib import Path
import select
import signal
import sys
import tempfile
import time
import uuid

KEYS = ("KEY_W KEY_A KEY_S KEY_D KEY_UP KEY_DOWN KEY_LEFT KEY_RIGHT KEY_SPACE KEY_LEFTSHIFT "
        "KEY_LEFTCTRL KEY_R KEY_X KEY_Z KEY_F KEY_G KEY_M KEY_H KEY_B KEY_T KEY_E KEY_Q "
        "KEY_1 KEY_2 KEY_ENTER KEY_ESC KEY_HOME KEY_EQUAL KEY_MINUS").split()
MOUSE_BUTTONS = ["BTN_LEFT", "BTN_RIGHT", "BTN_MIDDLE"]
PAD_BUTTONS = ("BTN_SOUTH BTN_EAST BTN_NORTH BTN_WEST BTN_TL BTN_TR BTN_TL2 BTN_TR2 "
               "BTN_SELECT BTN_START BTN_MODE BTN_THUMBL BTN_THUMBR").split()
PAD_AXES = {name: (-32768, 32767, 0) for name in ("ABS_X", "ABS_Y", "ABS_RX", "ABS_RY")}
PAD_AXES.update({name: (-1, 1, 0) for name in ("ABS_HAT0X", "ABS_HAT0Y")})
# gilrs 0.11.2 default Linux mapping: HAT2X -> RightTrigger2, HAT2Y -> LeftTrigger2.
# ABS_Z/RZ instead become LeftZ/RightZ without a vendor-specific SDL mapping.
PAD_AXES.update({name: (0, 255, 0) for name in ("ABS_HAT2X", "ABS_HAT2Y")})
DEVICES = {"keyboard", "mouse", "pad1", "pad2", "pad3", "pad4"}


def validate_command(command: dict) -> dict:
    if not isinstance(command, dict):
        raise ValueError("Each command must be an object")
    op = command.get("op")
    if op not in {"create", "events", "drop", "wait", "finish"}:
        raise ValueError("op must be create, events, drop, wait or finish")
    if set(command) - {"op", "device", "events", "seconds"}:
        raise ValueError("Unknown command fields")
    if op == "wait":
        seconds = command.get("seconds", 0)
        if isinstance(seconds, bool) or not isinstance(seconds, (float, int)) or not math.isfinite(seconds) or not 0 <= seconds <= 10:
            raise ValueError("wait.seconds must be finite, 0..10")
        return command
    if op == "finish":
        return command
    device = command.get("device")
    if device not in DEVICES:
        raise ValueError("Unknown device")
    if op != "events":
        return command
    events = command.get("events")
    if not isinstance(events, list) or not 1 <= len(events) <= 16:
        raise ValueError("An event burst needs 1..16 events")
    for event in events:
        if not isinstance(event, list) or len(event) != 3:
            raise ValueError("Event is [EV_TYPE, CODE, integer value]")
        kind, code, value = event
        if not isinstance(value, int) or isinstance(value, bool):
            raise ValueError("Event value must be an integer")
        if device == "keyboard":
            valid = kind == "EV_KEY" and code in KEYS and value in (0, 1, 2)
        elif device == "mouse":
            valid = ((kind == "EV_KEY" and code in MOUSE_BUTTONS and value in (0, 1)) or
                     (kind == "EV_REL" and code in ("REL_X", "REL_Y") and abs(value) <= 1200) or
                     (kind == "EV_REL" and code == "REL_WHEEL" and abs(value) <= 4))
        else:
            valid = ((kind == "EV_KEY" and code in PAD_BUTTONS and value in (0, 1)) or
                     (kind == "EV_ABS" and code in PAD_AXES and PAD_AXES[code][0] <= value <= PAD_AXES[code][1]))
        if not valid:
            raise ValueError(f"Unsupported or out-of-bounds {device} event: {event}")
    return command


def process_identity(pid: int) -> dict:
    if pid <= 1:
        raise ValueError("Explicit target PID must be greater than 1")
    proc = Path(f"/proc/{pid}")
    if proc.stat().st_uid != os.getuid():
        raise ValueError("Target must belong to the current user")
    exe = os.readlink(proc / "exe")
    if Path(exe.removesuffix(" (deleted)")).name != "panzerrechnersystem":
        raise ValueError("Target executable must be panzerrechnersystem")
    # /proc stat's comm field can contain spaces/parentheses; fields after its closing ')' start at3.
    rest = (proc / "stat").read_text().rsplit(")", 1)[1].split()
    return {"pid": pid, "start_ticks": int(rest[19]), "exe": exe}


class KWinFocusGuard:
    """Fresh compositor query per burst; the script only reads activeWindow and returns a nonce.

    KWin has no direct active-PID method. queryWindowInfo is an interactive picker and is not used.
    The temporary script is unloaded in finally; it never sets an active window or installs keys.
    """
    def __init__(self, pid: int):
        import dbus
        import dbus.service
        from dbus.mainloop.glib import DBusGMainLoop
        from gi.repository import GLib
        DBusGMainLoop(set_as_default=True)
        self.dbus, self.context = dbus, GLib.MainContext.default()
        self.bus = dbus.SessionBus()
        self.owner = self.bus.get_name_owner("org.kde.KWin")
        self.scripting = dbus.Interface(self.bus.get_object("org.kde.KWin", "/Scripting"), "org.kde.kwin.Scripting")
        self.identity = process_identity(pid)
        self.reply_handler = None
        guard = self

        class Reply(dbus.service.Object):
            @dbus.service.method("org.kessel.NativeInputGuard", in_signature="s", out_signature="", sender_keyword="sender")
            def Report(self, payload, sender=None):
                if guard.reply_handler is not None:
                    guard.reply_handler(payload, sender=sender)

        self.reply = Reply(self.bus, "/org/kessel/NativeInputGuard")

    def query(self) -> dict:
        if process_identity(self.identity["pid"]) != self.identity:
            raise RuntimeError("Target exited, changed executable or PID was reused")
        if self.bus.get_name_owner("org.kde.KWin") != self.owner:
            raise RuntimeError("KWin restarted; refusing a stale guard")
        token = uuid.uuid4().hex
        plugin = "kessel_native_guard_" + token
        result = []

        def report(*args, **kwargs):
            if kwargs.get("sender") != self.owner:
                return
            for arg in args:
                try:
                    data = json.loads(str(arg))
                except (ValueError, TypeError):
                    continue
                if isinstance(data, dict) and data.get("nonce") == token:
                    result.append(data)

        # New script per query prevents cached activation state from authorizing a later burst.
        source = ('var w=workspace.activeWindow; callDBus(' + json.dumps(self.bus.get_unique_name()) +
                  ',"/org/kessel/NativeInputGuard","org.kessel.NativeInputGuard","Report",' +
                  'JSON.stringify({nonce:' + json.dumps(token) +
                  ',pid:w?Number(w.pid):0,window:w?String(w.internalId):""}));')
        self.reply_handler = report
        loaded = False
        with tempfile.TemporaryDirectory(prefix="kessel-native-guard-") as temp:
            path = Path(temp) / "guard.js"
            path.write_text(source)
            try:
                # KWin exports one- and two-string overloads. dbus-python's introspection cache
                # can choose the one-string signature even when two arguments are supplied.
                # Specify the documented overload explicitly instead of relying on that cache.
                script_id = int(self.bus.call_blocking("org.kde.KWin", "/Scripting",
                    "org.kde.kwin.Scripting", "loadScript", "ss", (str(path), plugin), timeout=2))
                if script_id < 0:
                    raise RuntimeError("KWin refused read-only guard script")
                loaded = True
                script_path = f"/Scripting/Script{script_id}"
                script = self.dbus.Interface(self.bus.get_object("org.kde.KWin", script_path), "org.kde.kwin.Script")
                script.run(timeout=2)
                deadline = time.monotonic() + 1.5
                while not result and time.monotonic() < deadline:
                    while self.context.pending():
                        self.context.iteration(False)
                    time.sleep(0.002)
                if not result:
                    raise RuntimeError("No authenticated fresh KWin active-window result")
                return result[-1]
            finally:
                self.reply_handler = None
                if loaded:
                    self.scripting.unloadScript(plugin, timeout=2)

    def require_target(self) -> dict:
        result = self.query()
        if result.get("pid") != self.identity["pid"]:
            raise RuntimeError(f"Focus guard stopped input: active PID {result.get('pid')} is not target {self.identity['pid']}")
        return result


class VirtualDevices:
    def __init__(self):
        import evdev
        self.evdev = evdev
        self.open = {}

    def create(self, name: str) -> dict:
        if name in self.open:
            raise ValueError(f"Device already exists: {name}")
        e = self.evdev.ecodes
        if name == "keyboard":
            caps = {e.EV_KEY: [getattr(e, k) for k in KEYS]}
        elif name == "mouse":
            caps = {e.EV_KEY: [getattr(e, k) for k in MOUSE_BUTTONS], e.EV_REL: [e.REL_X, e.REL_Y, e.REL_WHEEL]}
        else:
            caps = {e.EV_KEY: [getattr(e, k) for k in PAD_BUTTONS],
                    e.EV_ABS: [(getattr(e, k), self.evdev.AbsInfo(n, lo, hi, 0, 0, 0)) for k, (lo, hi, n) in PAD_AXES.items()]}
        device = self.evdev.UInput(caps, name=f"Kessel AI validation {name}",
            phys=f"kessel-ai/native-validation/{name}", vendor=0, product=0,
            version=1, bustype=e.BUS_VIRTUAL)
        self.open[name] = device
        return {"device": name, "name": device.name, "event_node": device.device.path if device.device else None,
                "mapping": "generic Linux fallback; synthetic identity, not physical Xbox/PlayStation"}

    def events(self, name: str, events: list) -> None:
        device = self.open[name]
        e = self.evdev.ecodes
        for kind, code, value in events:
            device.write(getattr(e, kind), getattr(e, code), value)
        device.syn()

    def drop(self, name: str) -> None:
        device = self.open.pop(name)
        e = self.evdev.ecodes
        try:
            buttons = KEYS if name == "keyboard" else MOUSE_BUTTONS if name == "mouse" else PAD_BUTTONS
            for code in buttons:
                device.write(e.EV_KEY, getattr(e, code), 0)
            if name.startswith("pad"):
                for code, (_, _, neutral) in PAD_AXES.items():
                    device.write(e.EV_ABS, getattr(e, code), neutral)
            device.syn()
        finally:
            device.close()

    def close(self) -> list:
        errors = []
        for name in list(self.open):
            try:
                self.drop(name)
            except Exception as exc:
                errors.append({"device": name, "error": str(exc)})
        return errors


def run(args) -> int:
    commands = None
    if args.scenario:
        data = json.loads(Path(args.scenario).read_text())
        commands = [validate_command(c) for c in data["commands"]]
        if len(commands) > 1000:
            raise ValueError("At most 1000 commands")
    if not args.execute and not args.check_focus:
        print(json.dumps({"validated_commands": len(commands or []), "input_injected": False,
            "next": "Use --execute --pid GAME_PID with a scenario or --interactive only after native-window coordination"}))
        return 0
    if args.pid is None:
        raise ValueError("--pid is required for focus check or execution")
    guard = KWinFocusGuard(args.pid)
    if args.check_focus:
        result = guard.query()
        print(json.dumps({"target": guard.identity, "active": result,
            "matches": result.get("pid") == args.pid, "input_injected": False}))
        return 0 if result.get("pid") == args.pid else 2
    if not args.receipt:
        raise ValueError("--receipt is required for execution")
    if not args.interactive and commands is None:
        raise ValueError("Execution requires --scenario or --interactive JSONL")
    if args.interactive and commands is not None:
        raise ValueError("Choose scenario or interactive, not both")
    out = Path(args.receipt)
    if out.exists():
        raise ValueError("Receipt exists; choose a new unique path")
    out.parent.mkdir(parents=True, exist_ok=True)
    receipt = {"schema": "kessel.native-input-injector.v1", "target": guard.identity,
        "source": "Linux evdev.UInput, real native delivery; outcome requires separate game audit",
        "max_seconds": args.max_seconds, "started_unix_s": time.time(), "events": [], "completed": False}
    started = time.monotonic()
    devices = VirtualDevices()

    def checked():
        if time.monotonic() - started > args.max_seconds:
            raise TimeoutError("Bounded native input deadline reached")
        return guard.require_target()

    def wait(seconds):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            checked()
            time.sleep(min(0.10, max(0, deadline - time.monotonic())))

    def apply(command):
        command = validate_command(command)
        active = checked() # also before create/drop: hotplug affects active controller ownership
        row = {"elapsed_s": time.monotonic() - started, "command": command, "active": active}
        receipt["events"].append(row)
        op = command["op"]
        if op == "create":
            row["created"] = devices.create(command["device"])
        elif op == "drop":
            devices.drop(command["device"])
        elif op == "events":
            devices.events(command["device"], command["events"])
        elif op == "wait":
            wait(command.get("seconds", 0))
        return op != "finish"

    def interrupted(signum, frame):
        raise KeyboardInterrupt(f"Signal {signum}; releasing virtual devices")

    old_handlers = {s: signal.signal(s, interrupted) for s in (signal.SIGTERM, signal.SIGINT, signal.SIGALRM)}
    old_timer = signal.getitimer(signal.ITIMER_REAL)
    signal.setitimer(signal.ITIMER_REAL, args.max_seconds)
    exit_code = 0
    try:
        checked()
        if commands is not None:
            for command in commands:
                if not apply(command):
                    break
        else:
            print("READY: JSONL commands on stdin; devices persist until drop/finish/focus loss/deadline", flush=True)
            buffered = b""
            finished = False
            while True:
                checked()
                if b"\n" not in buffered:
                    ready, _, _ = select.select([sys.stdin], [], [], 0.10)
                    if not ready:
                        continue
                    chunk = os.read(sys.stdin.fileno(), 65536)
                    if not chunk:
                        if buffered.strip():
                            apply(json.loads(buffered))
                        break
                    buffered += chunk
                    if len(buffered) > 65536:
                        raise ValueError("Interactive input buffer exceeded 64 KiB")
                while b"\n" in buffered:
                    line, buffered = buffered.split(b"\n", 1)
                    if line.strip() and not apply(json.loads(line)):
                        finished = True
                        break
                if finished:
                    break
        receipt["completed"] = True
    except (Exception, KeyboardInterrupt) as exc:
        receipt["failure"] = str(exc)
        print(str(exc), file=sys.stderr)
        exit_code = 1
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)
        # Releases are permitted even if focus changed: never leave our held controls latched.
        receipt["cleanup_errors"] = devices.close()
        if receipt["cleanup_errors"]:
            receipt["completed"] = False
            exit_code = 1
        receipt["elapsed_s"] = time.monotonic() - started
        out.write_text(json.dumps(receipt, indent=2) + "\n")
        for s, previous in old_handlers.items():
            signal.signal(s, previous)
        signal.setitimer(signal.ITIMER_REAL, *old_timer)
    return exit_code


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pid", type=int, help="Explicit live panzerrechnersystem PID; active KWin PID must match")
    parser.add_argument("--scenario", help="JSON object containing a commands array")
    parser.add_argument("--interactive", action="store_true", help="Persistent devices, commands from JSONL stdin")
    parser.add_argument("--execute", action="store_true", help="Actually create virtual devices and send events")
    parser.add_argument("--check-focus", action="store_true", help="Read active KWin PID only; no uinput device")
    parser.add_argument("--max-seconds", type=float, default=60.0)
    parser.add_argument("--receipt", help="Unique output JSON, required for execution")
    args = parser.parse_args()
    if not math.isfinite(args.max_seconds) or not 1 <= args.max_seconds <= 120:
        parser.error("max-seconds must be finite and 1..120")
    if args.execute and args.check_focus:
        parser.error("check-focus never executes input")
    try:
        return run(args)
    except Exception as exc:
        print(str(exc), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
