# LAN crew co-op on Windows laptops

One Tiger, one crew, several Windows laptops on the same Wi-Fi. The host's laptop runs the
simulation; every other laptop sends its station's controls and receives the world 20 times a
second (`docs/LAN_COOP.md` has the protocol and the station rules). This page is the Windows
setup, the firewall, troubleshooting, and an honest list of what was verified and what is not.

## Quick start

1. **Make the portable folder** on the machine that holds the asset library (once per build):

   ```powershell
   powershell -ExecutionPolicy Bypass -File tools\package_windows.ps1
   ```

   It builds the release exe and fills `dist\Panzerrechnersystem-win64\` (about 2.2 GB). Copy the
   whole folder to each laptop by USB stick, external drive or a share on your own network.
   **The folder is private**: the models and sounds are purchased kits licensed to you. Copy it to
   your own machines only; never upload it or put it on a public share. `/dist/` is git-ignored.

2. **On the host laptop, once:** double-click **`Allow LAN in firewall.cmd`** (or run
   `tools\setup_firewall.ps1`) and accept the UAC prompt. It adds two inbound rules for this exe,
   Private networks only (details below).

3. **Host:** double-click **`Host Canyon.cmd`** (or `Host Campaign.cmd`, `Host Arena.cmd`). Until
   somebody joins, the top-left of the screen reads `FRIENDS JOIN 192.168.1.50 (Wi-Fi)`. The same
   address is on the title menu's Crew co-op page (`THIS PC ...`).

4. **Friends:** double-click **`Join.cmd`** and type that address, or `auto` to take the first
   compatible Tiger heard on the network. The title menu's Crew co-op page also lists nearby Tigers.

From a checkout instead of the folder, the same launcher:

```powershell
powershell -ExecutionPolicy Bypass -File tools\launch_windows.ps1 -Host Canyon
powershell -ExecutionPolicy Bypass -File tools\launch_windows.ps1 -Join 192.168.1.50 -Role gunner
powershell -ExecutionPolicy Bypass -File tools\launch_windows.ps1 -Join auto -Laptop -Windowed
```

`-Host Campaign|Arena|Garage|Canyon`, `-Join <address>|auto`, `-Role commander|driver|gunner|
loader|radio` (the host arbitrates; F6 asks for the next free seat), `-Name` (default: the Windows
user name), `-Port` (default 28460), `-Seed` (host), `-Tier auto|laptop|4090`, `-Laptop`,
`-Windowed`, `-Renderer shader|raymarch`, `-Resolution`.

## The crew

The host sits at Commander; friends take the next free seat. **The living AI crew mans every seat
nobody holds**: the gunner lays on contacts and fires under the standing order (K toggles fire at
will / hold fire), the driver follows the commander's WASD with a man's reaction time, the loader
serves the breech at a man's rate (about six seconds a round). A crewman steps aside the moment a
human is given his seat or stands at his post, and takes it back when the human leaves the post,
dismounts or disconnects. Dead men stay dead: a killed loader's breech waits for a human hand. All
of it is decided on the host; clients see the AI men at their posts through the world packet's
`ai_crew_mask` (protocol 9).

Two or three humans with the AI crew is the intended small-group game: for example host commands,
one friend drives, the AI gunner and loader fight.

## Windows Firewall

The host listens on **UDP 28460** (gameplay) and every copy listens on **UDP 28461** (discovery,
the "nearby Tigers" list). An unsigned exe gets no inbound traffic until the firewall allows it.

**`tools\setup_firewall.ps1`** adds exactly two rules, both bound to the game's exe and to the
**Private** profile:

| Rule | Direction | Protocol / port | Program | Profile |
|---|---|---|---|---|
| Panzerrechnersystem LAN game (UDP 28460) | Inbound, allow | UDP 28460 | this `panzerrechnersystem.exe` | Private |
| Panzerrechnersystem LAN discovery (UDP 28461) | Inbound, allow | UDP 28461 | this `panzerrechnersystem.exe` | Private |

It changes nothing else: no other rule, no profile, no network category. It re-runs itself
elevated through `Start-Process -Verb RunAs` (one UAC prompt). Re-run it after moving the folder
(the rules name the exe's path). Options: `-Check` (report, no changes, no elevation), `-Remove`,
`-RemoveBlockRules` (see below), `-Exe <path>`.

**The Windows prompt.** The first time the game hosts, Windows shows *"Windows Security Alert:
Windows Defender Firewall has blocked some features of this app"* with checkboxes for Private and
Public networks. Tick **Private**, untick Public, click **Allow access** (needs an administrator
password on a standard account). That is equivalent to the script. **Cancel is not "ask later"**:
it creates *block* rules for the exe, and a block beats any allow. `setup_firewall.ps1
-RemoveBlockRules` deletes only the inbound block rules whose program is this exe.

**Private vs Public.** Both rules apply to Private networks only. If Windows calls your home Wi-Fi
*Public*, open Settings > Network & internet > Wi-Fi > (your network) and choose **Private
network**. The script reports the category (`-Check`) but never changes it.

Clients normally need no rule: the host's replies are answers to the client's own traffic. A
client only needs the discovery rule for the browse list; joining by address works without it.

## Troubleshooting

A client that gets no Welcome within 4 seconds shows the reason and what to check, without F3:

| On screen | Meaning | What to do |
|---|---|---|
| `NO ANSWER FROM 192.168.1.50:28460` | nothing came back | same Wi-Fi? host ran `setup_firewall.ps1` (or allowed Private at the prompt)? guest network or *AP/client isolation* (hotel, office, some mesh "guest" modes block laptop-to-laptop traffic; a phone hotspot works)? the address from the host's screen, not a `vEthernet`/VPN one? |
| `192.168.1.50 IS UP BUT NOT HOSTING ON UDP 28460` | the host machine answered "port unreachable" | the host has not started hosting yet, or uses another `-Port` |
| `TANK FULL - FIVE CREW ALREADY ABOARD` | five humans aboard | wait for a seat |
| `HOST LOST - RECONNECTING` | no packet for 5 s after joining | the host quit or the Wi-Fi dropped; the client keeps knocking |

The same checklist is printed to the console (`[lan] ...`, in the launcher's log under
`%LOCALAPPDATA%\Panzerrechnersystem` or `artifacts\runtime`).

**Diagnostics without the game window** (no GPU, safe on any laptop):

```powershell
panzerrechnersystem.exe lan-ifaces                  # this laptop's IPv4 adapters, broadcasts, the address to give friends
panzerrechnersystem.exe lan-probe discover 4        # list the Tigers heard on UDP 28461
panzerrechnersystem.exe lan-probe join 192.168.1.50 # handshake + snapshot rate against a real host, or the checklist
panzerrechnersystem.exe lan-probe host 120          # a stand-in host (no window): friends can lan-probe join it
powershell -ExecutionPolicy Bypass -File tools\setup_firewall.ps1 -Check
```

`lan-probe join` exits 0 only when the Welcome arrived and snapshots streamed; it reports the
Welcome time and the snapshot rate (the host sends 20/s).

**Several adapters.** Laptops with WSL, Hyper-V, Docker, VirtualBox or a VPN carry extra IPv4
adapters (`vEthernet (WSL)`, `vEthernet (Default Switch)`, `Tailscale` ...). A plain
255.255.255.255 broadcast leaves on only one adapter, often a virtual switch. The game sends its
beacon to every adapter's directed broadcast (`192.168.1.255` for `192.168.1.50/24`) plus the
limited broadcast, re-reads the adapters every 10 seconds, and offers friends the default-route
Wi-Fi/Ethernet address first; virtual and VPN adapters are marked and never offered while a
physical one exists. If discovery still fails, join by address.

## Laptop GPUs

The launcher's `-Tier auto` uses the 4090 profile only on a desktop RTX 4080/4090/5080/5090, and
the **laptop profile** everywhere else: internal grid **480x272** (the engine's nearest grid to
480x270; it has no 480x270), the `laptop` preset, `gameplay` transport sampling, no MSAA, the
shader backend, or raymarch when a ray-tracing GPU is present (RTX 20-50, Radeon RX 6000/7000, Arc
A). The laptop profile keeps its own saved preferences (`graphics.laptop.v1.json`). It changes
resolution and sampling only, never the scene or the simulation (CLAUDE.md, second principle).

Measured on this machine's **Intel UHD Graphics 770** (the desktop's integrated GPU, Vulkan, no
ray queries), Canyon v2, buttoned commander view, 60 timed frames of `winter-bench` (fenced
offscreen frames, `KESSEL_ADAPTER=UHD` selects the iGPU; receipts in `artifacts/lan-windows/`):

| GPU | Grid | Backend | Frame p50 | GPU wait p50 | Simulation p50 | Work per frame |
|---|---|---|---|---|---|---|
| Intel UHD 770 | 480x272 | shader, laptop, gameplay, no MSAA | 3,008 ms | 2,962 ms | 3.1 ms | 5 scene renders, 31 shadow passes, 5,060 draws |
| Intel UHD 770 | 320x180 | shader, laptop, gameplay, no MSAA | 3,004 ms | 2,945 ms | 3.4 ms | the same |

The frame rendered correctly on Intel's driver, but about 0.3 frames per second is unplayable.
Nearly all of the time is GPU work that does not scale with the grid: a third fewer pixels saved
nothing, so the cost is the shadow passes and draws per view, not fill. **Integrated graphics are not enough for this game** (Intel UHD/Iris Xe, Radeon 680M/780M
class); each laptop needs a discrete GPU. Discrete laptop GPUs (RTX 3050-4070 Laptop) were not
available to measure; my assumption is that a mid-range RTX laptop GPU runs the laptop profile at
playable rates, since the same frame's draws and shadow passes are what an RTX desktop card handles
in the shipping profile, but that is unverified. A 4090 reference at the laptop settings could not
be measured cleanly: the card was at 100 % from other processes during the run.

## What the snapshot carries, and what is not replicated

Host-authoritative and sent 20 times a second: the Tiger (position, heading, speed, turret, gun,
reload, hull points, alive), up to 24 enemy tanks (the same fields; kills are the host's), engine,
every hatch and the driver's visor, breech state, rounds fired (each discharge is presented once on
clients), HE/AP loaded, cabin fire level, the 16 module states, crew alive mask, the AI crew mask,
act and act-complete, the armor scars (a rotating window of 16 records), and the full crew frame
(who sits where, everyone's station controls and physical post). Building damage, doors and
containers travel in their own packets. Static scatter and the authored scene are rebuilt
identically from the same manifest and seed on every machine.

**Not replicated yet** (the client keeps its own local version or does not show it):

- **Wounds, bleeding, first aid and the medkit procedures** (`first_aid`, `crew_solo`): solo only.
  In LAN a penetration kills crewmen through the crew alive mask; there are no LAN wounds, no
  treatment, no body to drag.
- **Intercom lines** of the AI crew: on the host's screen only.
- **Ammunition stowage**: which rack slots are empty, loose and dropped rounds, carried rounds,
  spent cases and casing stowage. Each machine simulates them locally from the replicated breech.
- **Mission beats and orders** (`mission_game`: order sheet, signal flares, smoke columns): host only.
- **Scatter that moved**: shoved or broken heavy scatter pieces after the first contact.
- **Platoon wingmen** (`KESSEL_PLATOON=2`): the host's AI Tigers are not in the snapshot; clients
  do not see them.
- **Infantry, AT guns and enemy fire**: clients run their own copies and are corrected only for
  tank positions and kills; damage to the Tiger always comes from the host.
- **Time of day and weather**: each machine's own menu setting.
- **Renderer choice** (shader/raymarch) and quality: local by design; both draw the same state.

## What was verified, and what is not

Verified on this Windows 11 machine (RTX 4090 + Intel UHD 770, Wi-Fi plus two Hyper-V switches),
branch `lan-windows`:

- `cargo test --bin panzerrechnersystem`: the LAN, discovery and interface tests pass (41), among
  them a closed loopback port read as `NotHosting` through Windows' `WSAECONNRESET`, a host that
  keeps serving after one client's port vanished, a discovery send that survives a refusing
  adapter, and the Wi-Fi-before-`vEthernet` ordering. Whole suite: no new failures against the
  base (the remaining failures read git-ignored assets from a worktree, as on `main`).
- `lan-verify`: PASS.
- `lan-ifaces` on this machine: Wi-Fi 192.168.1.228/24 first (default route), `vEthernet (Default
  Switch)` and `vEthernet (WSL (Hyper-V firewall))` classified virtual; beacons to 192.168.1.255,
  172.17.31.255, 172.27.79.255 and 255.255.255.255.
- **Two processes over loopback** (`lan-probe host` + `lan-probe join 127.0.0.1`): Welcome in 6 ms,
  92 snapshots in 5 s (18.6/s; the host sends 20/s), gunner input received, callsign from `USERNAME`.
- `tools\setup_firewall.ps1 -Check` and the launcher's `-Mode check` (both tiers): run and report
  correctly; scripts parse in Windows PowerShell 5.1.
- **The portable folder** (`tools\package_windows.ps1`): 873 files, 2.09 GB (2,192 MB decimal).
  The closure is the locked catalogs, every manifest and runtime catalog the shipping source names
  (walked recursively), the enabled soundscape clips, and a committed load trace of all six
  hostable worlds (`tools\windows_payload_traces\`, file names only). The packaged exe, started
  from the folder with no `KESSEL_ASSET_ROOT`, found its assets by itself and built the garage,
  arena, acts 1-3 and the Canyon v2 course (`asset-trace`, Intel UHD 770); its load trace shows
  **no file missing** against the full 3.4 GB library. Two finds fixed on the way: tracked files
  (mission sheets) ship from the building checkout, not an older asset checkout, and the terrain
  detail tiles under `source_assets/` are runtime loads.

**Not verified yet** (needs the owner's GPU window, a UAC click, or a second laptop):

- A real windowed game host and client (the GPU hold allowed only short headless processes).
- Anything over the Wi-Fi address: binding a non-loopback port raises the Defender prompt on the
  owner's desktop, so it waits for the owner: run `tools\setup_firewall.ps1`, then
  `panzerrechnersystem.exe lan-probe host 60` and, in a second window,
  `panzerrechnersystem.exe lan-probe join 192.168.1.228`, then the same with two game windows.
- Two physical laptops, client isolation on a real access point, and discovery across them.
- The AI crew in a live LAN fight (built and unit-tested paths only; no live capture yet).
- Discrete laptop GPU frame rates (see "Laptop GPUs").
