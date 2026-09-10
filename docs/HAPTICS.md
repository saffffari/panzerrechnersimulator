# Panzerrechnersystem tactile feedback

The mouse and controller adapters share completed simulation events from
`src/haptics.rs`. Input presses alone do not trigger feedback. The live loop must
compare snapshots immediately around an update of the same world, and silence
both adapters while paused, unfocused, in menus, or leaving that world.

## MX Master 4 on HAL Linux

`src/mouse_haptics.rs` uses the existing Keys service at
`/run/keysd-haptics.sock`. It does not open HID devices, start services, change
button bindings, or write device configuration. Construction performs no I/O.
Each accepted cue sends one named finite waveform without a strength override,
so the service retains the user's configured strength.

| Simulation event | Keys waveform |
| --- | --- |
| Main gun fired | `sharp_collision` |
| Own tank hit, penetrating | `damp_collision` |
| Own tank hit, nonpenetrating | `subtle_collision` |
| Main gun misfire | `sharp_state_change` |
| Round loaded | `damp_state_change` |

`MouseHaptics::new(enabled)` constructs the adapter; `pulse(cue)` attempts one
nonblocking datagram. It returns transport success, not confirmation that the
mouse was felt. Requests closer than 125 ms are dropped. A missing, busy, or
restarted service produces no user-facing error; a later new cue may retry after
one second. There are no timers or local playback queues, and lost cues are
never replayed. Non-Linux platforms use a no-op backend.

`stop()` disables future requests and closes this adapter's socket; `resume()`
allows new requests only if the adapter was enabled at construction. Neither
method sends a command. The existing Keys IPC has **no cancellation command**,
so a finite effect already accepted by that service can finish after a pause.
Cooldown and reconnect limits survive stop/resume. These effects are brief
event feedback, not continuous engine vibration.

## Evidence and acceptance

On 2026-09-09, this boot's LogiOps journal identified **MX Master 4** and the Keys
daemon logged its haptics endpoint ready at configured strength 45. The Unix
socket existed. This verifies device discovery and an available local interface;
it does not by itself prove current physical playback.

The protocol was checked against the installed Keys Python IPC client and
`/home/alex/keys/daemon/src/haptics.rs`. Upstream
[Solaar's capabilities documentation](https://github.com/pwr-Solaar/Solaar/blob/master/docs/capabilities.md)
also documents named MX Master 4 haptic playback on Linux. Logitech's
[supported-device list](https://logitech.github.io/actions-sdk-docs/supported-devices/)
includes MX Master 4 haptics, but its
[Actions SDK plugin host](https://logitech.github.io/actions-sdk-docs/plugin-basics/)
lists Windows and macOS; the game therefore uses HAL's existing local Linux
service.

The shared cue module and mouse adapter passed **10 standalone CPU tests**:
four event tests and six adapter tests, including rate limiting, disabled output,
pause/resume, missing service, and a simulated service restart. All adapter tests
use private temporary Unix sockets; none accesses the live mouse endpoint.
No Cargo build, GPU run, service change, or physical test pulse was performed as
part of this adapter work. Live game wiring, a felt-event check, and confirmation
that pause/focus loss prevents subsequent feedback remain runtime acceptance
steps for the integrating task.
