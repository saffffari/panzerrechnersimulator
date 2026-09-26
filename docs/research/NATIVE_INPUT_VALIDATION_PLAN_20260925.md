# Native input validation — 25 September 2026

Athena (AI). **Native keyboard delivery and a two-seat co-op virtual-controller run were validated on the actual KDE/winit/gilrs path.** The keyboard review passed 12/12 state checks; the co-op review passed 14/14 checks and a visually inspected split-screen capture. These are bounded historical receipts, not certification of every controller or mechanic. The original feasibility inspection below preceded these coordinated runs. No permission changes or persistent daemon were needed.

## Host state at the initial feasibility inspection

- KDE Wayland session, `WAYLAND_DISPLAY=wayland-0`, `DISPLAY=:0`.
- Outside the restricted command sandbox, `/dev/uinput` has an effective `user:alex:rw-` ACL. Both read and write access checks succeeded for UID 1000. The sandbox hides the device node; that is distinct from a host permission failure.
- System Python has `evdev`; `wtype`, `ydotool` and `wayland-info` are installed. `evtest` and `evemu-device/event` are absent. No installation is needed for an `evdev.UInput` implementation.
- `/proc/bus/input/devices` identifies `js0` as **Mouse passthrough (absolute)**, not a physical gamepad. No physical gamepad was identified at that initial inspection. During the later co-op run a separate device named **PS5 Controller** connected and produced input; it was not created by this test tool. This change is disclosed in the co-op receipt, and physical-controller compatibility was not certified.
- Read-only `wayland-info` completed successfully. This session did **not** advertise `zwp_virtual_keyboard_manager_v1`. Upstream [wtype source](https://github.com/atx/wtype/blob/master/main.c) requires that interface. Therefore wtype is not the proposed route for this compositor. It was not executed to send keys.

[Linux uinput](https://docs.kernel.org/input/uinput.html) creates a temporary kernel input device with declared capabilities and delivers its events through the ordinary input stack. [Python-evdev](https://python-evdev.readthedocs.io/en/latest/tutorial.html#injecting-input) already exposes that facility here. Use one scoped process with explicit release/neutral events and a `finally`/context-manager close; do not install a persistent daemon or alter ACLs.

## What existing tests cover

`src/gamepad.rs` uses the real `gilrs::Gilrs` backend. `poll_all` drains backend events, enumerates connected pads, retains per-pad edges, assigns stable namespaced IDs and selects the active pad. `read_frame` applies the 0.18 radial stick deadzone, maps the RT/LT levels, and generates one-frame map/interact/menu edges. Held-at-discovery buttons are baselined rather than treated as new presses. Three source-local tests exercise deadzone, initial edge baseline and peer IDs, **without creating a real backend device**.

`src/main.rs` has extensive pure couch routing tests around `couch_pad_role_input`, ownership and disconnect behavior. Its actual winit `KeyboardInput`/`MouseInput`/`DeviceEvent::MouseMotion` handlers remain a separate boundary. Focus loss clears held keys and pauses solo play; focus regain drains controller edges. These branches require native-window evidence to establish end-to-end behavior.

`mechanic-review` and `mechanic-evaluate` feed `TankInput`/actions into ordinary Game simulation and can prove many downstream mechanics, render and audio outcomes. They **bypass winit events and the gilrs device backend**. A successful film cannot be labelled a native keyboard/gamepad test.

No existing native-event injection/receipt harness was found in `src`, `tools` or the inspected docs at the initial inspection. The staged tool described below was then prepared at the root agent's request. A useful separate backend probe can compile the actual `src/gamepad.rs`, `src/controller_haptics.rs` and `src/haptics.rs` as modules, with the existing gilrs dependency, and log `Gamepads::poll_all()` frames. Keep haptics disabled. A replacement fake `PadFrame` would miss the boundary being tested.

## Broader proposed matrix (only the bounded results below are complete)

1. **Backend probe, no window/GPU, about 15 seconds.** Root first coordinates with any live game: gamepad hotplug is global and can affect an existing session. Start the real-module probe, then create two explicitly named test devices, e.g. `Kessel AI validation pad A/B`, with neutral initial state and synthetic IDs. Wait for both actual gilrs `Connected` observations before sending input. Assert controls on pad B do not alter pad A; remove A while B is held; verify B's peer ID and its release remain stable; close both devices. Save timestamped native events, mapped `PadFrame`s, UUIDs, capabilities and checks in an `artifacts/runtime/native_input_...json` receipt.
2. **Native keyboard/mouse and gamepad pilot, one visible game window, 30–60 seconds.** With a coordinated fixed game build and known solo test state, create separate temporary keyboard/mouse capabilities and the validated pad. Use the game's normal menu/controls. Capture the visible end state and a narrowly scoped event/state trace. Verify M map open/close, movement press/release, relative mouse look, RT/LMB contextual action, RMB/LT cupola pistol raise, and pause/focus-loss release. Verify that Space/A traversal does not fire the cannon and a held entry click needs a release/new press before firing.
3. **Couch ownership pilot only after the first two pass.** Two pads are enough for stable identity/removal tests; four temporary pads can exercise the real backend's full roster without physical controllers. Verify keyboard Commander versus actual physical Gunner station authority, no forced Commander seat every redraw, and disconnect/reconnect preservation. No generated `CrewFrame` can substitute for the native backend in this pilot.

Bound each phase with a timeout. All generated devices must close on success or failure; send neutral axes and all released buttons first. Do not grab existing physical input devices. An injector process creating a pad while the user plays can change active-controller/crew authority even before the first button event, so coordinate the window/session before device creation.

## Device mapping details that matter

The installed versions are `gilrs 0.11.2` / `gilrs-core 0.6.8`, inspected from the local Cargo registry. The Linux backend's udev route filters `ID_INPUT_JOYSTICK=1`, then checks capabilities. Device creation is not proof of recognition: wait for the actual backend event and record the udev properties.

For a generic Linux pad, declare `ABS_X/Y`, `ABS_RX/RY`, `ABS_HAT0X/Y`, normal gamepad face/shoulder/stick/menu buttons, and `BTN_TL2/TR2` for the first digital-trigger pass. Use correct axis ranges/neutral values and `SYN_REPORT` boundaries. Do not impersonate an Xbox or PlayStation product ID merely to select a database mapping.

**Do not assume generic `ABS_Z/RZ` become RT/LT.** This version's default fallback maps those to `LeftZ/RightZ`; `read_frame` reads `LeftTrigger2/RightTrigger2` buttons. The fallback maps `ABS_HAT2X/Y` to second triggers, while a specific SDL database mapping can map Z/RZ differently. For a second analog-trigger test, use an explicitly disclosed process-local mapping or the inspected fallback trigger axes, and verify the actual semantic output around the game's 0.24 threshold. Do not report a generic virtual pad as a physical Xbox compatibility result.

Native code currently maps South/A to held `jump` and edge `interact_pressed`, West/X to reload, North/Y to dismount hold, Select/View to map edge, RT to primary, LT to aim/zoom edge, RB to coax, and stick clicks to crouch/ignition. The route into the current station determines the final action. Mouse movement must use `EV_REL`, not absolute-pointer warps, to exercise the raw-look path.

## Honest completion boundary

The completed bounded runs establish keyboard → compositor → winit → Game behavior and virtual uinput → evdev/udev → gilrs → co-op Game behavior for the specific checks recorded below. The broader proposed matrix remains incomplete. It cannot certify physical stick calibration, Bluetooth latency, vendor-specific mapping quirks, motor feel or hardware disconnection behavior. Haptic API receipts can verify requested events separately; they do not prove felt rumble.

One stale user-facing line was found: the opening embodied-controls description in `docs/CONTROLS.md` still says exposed-Commander RMB toggles binoculars; current native code raises the P08 and leaves B for binoculars. This read-only pass did not edit that document.

## Guarded tool and reusable scenarios

`tools/native_input_probe.py` was exercised during the native runs. Its default mode validates only; `--execute` is mandatory for input. It requires an explicit PID whose executable is `panzerrechnersystem`, pins that process's start time, and refuses PID reuse or a different executable. Before every burst and during waits it queries KWin's current active PID using a temporary read-only script and an authenticated D-Bus callback. The script only reads `workspace.activeWindow`, returns a nonce/PID/window identifier to this process’s unique bus address, and is unloaded immediately. The callback rejects a sender other than KWin’s current unique bus owner and rejects stale nonces. The injector calls the overloaded `loadScript` method with explicit `ss` argument signature; relying on introspection selected the wrong one-argument overload on this host. KWin’s advertised print signal did not arrive reliably, so it is not the guard transport. It neither activates windows nor installs shortcuts. See [the KWin scripting API](https://develop.kde.org/docs/plasma/kwin/api/) for these interfaces. The PID query succeeded at runtime. Mismatched-focus attempts also failed before device creation; their receipts are retained.

The injector has a 60-second default/120-second maximum timer, command bounds, all-key release/axis neutralization, and device close on errors, signal, focus loss or EOF. This minimizes mistaken delivery; checking focus and sending an event cannot be atomic across separate compositor/kernel APIs, so there remains a small check-to-send race. Generated device names explicitly say “Kessel AI validation”; they advertise synthetic vendor/product 0 and no force-feedback capabilities. It does not grab or alter physical devices.

Tool CPU evidence: `python -m unittest discover -s tools -p 'test_native_input_probe.py' -v` passed **6/6**. Tests cover accepted/rejected event bounds, wrong-PID refusal before device creation, cleanup neutralization, cleanup after a failed release write and default dry-run behavior. The keyboard scenario validated 36 commands; pad scenario validated 27. Neither creates a device during validation.

For any later coordinated run, first check the newly verified PID:

```bash
python tools/native_input_probe.py --pid GAME_PID --check-focus
```

It reports the active PID and match result without opening uinput. On a wrong focus it returns status 2. If no authenticated fresh KWin result arrives, execution fails closed. Only after that works and the starting scene is verified, execute one staged scenario with a fresh receipt path:

```bash
python tools/native_input_probe.py --pid GAME_PID --execute --max-seconds 30 \
  --scenario catalog/native_inputs/keyboard_commander_20260925.v1.json \
  --receipt artifacts/runtime/native_keyboard_20260925_v1.json
```

The solo pad scenario is `catalog/native_inputs/pad_solo_commander_20260925.v1.json` and remains unexecuted. The successful co-op commands are preserved separately in `catalog/native_inputs/pad_coop_paused_commander_20260925.v1.json`; that case explicitly starts at a visibly verified pause menu and resumes it before testing. Each JSON lists explicit prerequisites and required observations. The original keyboard and solo-pad cases begin in already-playing, alive, buttoned Commander solo mode; they do not navigate menus or force a setup state. Space in this setup descends to Loader; head-out Space has a different traversal and is intentionally not conflated with it. The injector receipt proves commands delivered after guard checks, **not their gameplay effects**. Compare root's `KESSEL_NATIVE_INPUT_AUDIT=1` state stream and visible frames before declaring a pass.

For a controller that must persist between solo and a later couch test in the **same game process**, run:

```bash
python tools/native_input_probe.py --pid GAME_PID --execute --interactive --max-seconds 120 \
  --receipt artifacts/runtime/native_persistent_pad_20260925_v1.json
```

Send JSONL commands on its still-open stdin, for example `{"op":"create","device":"pad1"}`, then bounded `events`; later `{"op":"create","device":"pad2"}` adds the second device without replacing pad1. `{"op":"finish"}` releases/closes all. A new game process requires a new explicit PID and run; the tool will not silently retarget across an executable restart. Interactive EOF closes devices. Further gameplay events must wait for a newly coordinated native session; the current user-owned game is not a test target.


## Completed native evidence

**Keyboard, actual Singleplayer process 569411.** `artifacts/runtime/native_keyboard_20260925_v2_review.json` records 12/12 checks against `artifacts/runtime/shader-playtest-20260925-191146-569399.log`. The simulation clock advanced, Commander LMB was observed without a cannon discharge, M opened/stowed the map, Equal/Minus changed zoom 1.00 → 1.18 → 1.00, Space descended to Loader and stayed there, W+Shift/W+Ctrl reached run/crouch inputs, and controls released with the player alive. This establishes input/state behavior, not sprint-speed calibration or an independently reviewed native map screenshot. The first `v1` attempt was paused and is not gameplay proof. A later map screenshot attempt rejected mismatched focus before injecting anything; retain `artifacts/runtime/native_map_open_20260925_v1.json` as failure evidence.

**Co-op, actual process 588009.** `artifacts/runtime/native_coop_20260925_v3_review.json` records 14/14 checks against `artifacts/runtime/shader-playtest-20260925-192235-587988.log`. Real gilrs connected `Kessel AI validation pad1`, peer 2147483648. At game time 31.773 s the virtual right stick produced look 0.11547. At 32.041 s its primary became true and shots increased 0 → 1 while the local player remained Commander with local primary/fire false. The virtual trigger was released by 32.307 s. At that discharge sample the separate PS5-named pad also had primary false. The virtual device later disconnected and the injector reported no cleanup errors.

`captures/astra/native_coop_20260925_v3_split.png` was visually inspected: the top view is labelled **P1 · COMMANDER · TASTATUR**, and the bottom view **P2 · GUNNER · PAD1**, looking through the gunsight into the winter courtyard. The filename `native_coop_20260925_v2_split.png` is misleading: that earlier image shows the pause menu and is explicitly not split-screen gameplay evidence. The `v1` co-op attempt rejected focus before any device creation. The co-op process was closed only after its PID, executable and start time were reverified; the GPU was then released. Both test processes are historical and must not be retargeted.

The co-op interval also contains a separate PS5-named controller and other local input. It was not an isolated hardware test. The discharge is attributable to the virtual peer at the sampled instant, but this does not establish physical PS5 compatibility or haptic behavior. Native co-op Commander LMB rejection, a four-pad roster/hotplug matrix, analog-trigger calibration and the newly implemented Main Menu/Restart Game lifecycle remain unvalidated in this pass. The new pause-menu native checks were deferred so the user could play uninterrupted.

The injector never focuses windows. An attempt to run an unexplained existing KWin script ID was rejected by automatic approval review and was not executed. The later explicit focus operation used newly inspected source restricted to the verified co-op PID and application class, received approval, and unloaded its script. No unknown script ID or blind Alt-Tab was used to recover focus.
