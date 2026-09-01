# Kessel LAN crew co-op

Status: playable in development builds. Local split-screen supports four people on one machine;
LAN supports all five Tiger posts. Protocol version 1 uses UDP **28460** for gameplay and UDP
**28461** for optional nearby-game discovery. Manual IP joining remains available if discovery is
blocked. A packaged two-Mac permission/firewall smoke test is still required before friend release.

## The shape of the game

Co-op means one Tiger, one physical simulation, up to five human stations:

| Station | Human job | When the seat is empty |
|---|---|---|
| Commander | observe, navigate, call the fight, cover missing controls | another occupied seat becomes command authority |
| Driver | WASD movement, steering, ignition | commander covers it only in a one/two-person tank |
| Gunner | mouse lay, 88 mm, coax MG, sight magnification | commander/first player may lay and fire |
| Loader | service the hot casing and ram the next round | gunner cross-trains at 2–3 crew; AI helps solo |
| Radio / bow MG | hull MG and map/intel watch | commander/first player may fire the bow MG |

This is why **two and three players are first-class configurations**, not crippled five-player
sessions. Two players claim Commander and Gunner: the commander also covers driving, while the
gunner physically moves between the sight and breech. Three players add a dedicated Driver; with
three humans aboard the tank accepts movement only from that occupied Driver post. Four add a
dedicated Loader; five put a human behind the radio and bow MG.

Connected crewmen are visible as low-poly tanker bodies at their actual reported posts. The HUD
adds a `>POST` suffix when someone has climbed away from their assigned role (for example `G:MAN>L`
while the gunner is loading). Your own body is hidden locally so the first-person camera stays clear.

The host is authoritative for physics, enemies, damage, modules, crew, ammunition, fire, campaign
progress and death. Clients transmit station input only and receive a compact world correction 20
times per second. A dropped UDP packet is harmless because newer input/state replaces it. A client
that loses the host automatically returns to the handshake loop; a vanished crewman is removed
after five seconds and his job immediately falls back to the remaining crew.

## Four-person local Stützpunkt

Choose **Crew co-op → Split-screen crew** for the self-contained TAKE & HOLD demo mission. Keyboard
and mouse own Commander; controllers 1–3 own Gunner, Driver, and Loader. The Radio/Bow-MG operator
is deliberately left out, giving all four friends a defined essential role without an extra MG-only
station. With fewer controllers the same fallback rules keep the tank operable, and hot-plugged pads
take the next seat immediately. The fight opens on three dug-in T-34s, then sends close waves of
four, five, and six with only short regroup beats.

The display uses two halves for two players, a tall Commander half plus stacked Gunner/Driver views
for three, and a 2×2 Commander/Gunner/Driver/Loader grid for four. If a crewman is KIA, that post
opens for reassignment rather than ending the mission immediately. A surviving controller player
presses **B (Circle)** to take over the next open KIA post; the keyboard player uses **F6**. The
survivor's old post becomes vacant, preserving the pressure of operating with a reduced crew.

Every surviving viewport receives the same directional spall/blood flash and KIA banner. The fallen
crewman's body and a restrained dark blood pool remain at the physical post where he was hit, even
after a survivor climbs over him to take control; only the viewing player's own living body is hidden.

## Launch on a LAN

From the title screen choose **Crew co-op**. The next page can:

- host Campaign, Garage, or Arena;
- join a nearby compatible Tiger that appears automatically;
- join a hostname/IP manually (the gameplay port is optional); or
- start local split-screen crew play.

Discovery is only a convenience. If macOS denies Local Network access, a firewall blocks UDP 28461,
or a Wi-Fi access point isolates clients, enter the host's LAN address manually. Gameplay still
requires inbound UDP 28460 on the host. The first packaged launch should explain and trigger the
normal macOS Local Network permission prompt.

The command-line path remains useful for development and diagnostics. On the host machine, from
the repository root:

```sh
cargo run --release -- --host --mission campaign --role commander --name OTTO --seed 1942
```

On another machine:

```sh
cargo run --release -- --join 192.168.1.50 --role gunner --name KARL
cargo run --release -- --join 192.168.1.50 --role driver --name ERNST
cargo run --release -- --join 192.168.1.50 --role loader --name HANS
cargo run --release -- --join 192.168.1.50 --role radio --name FRITZ
```

The host's mission and seed override the values requested by clients. `garage`, `arena`, and the
complete three-act `campaign` are valid missions. Add `--bind 0.0.0.0:PORT` on the host and use
`HOST:PORT` on clients to choose another UDP port. If a firewall is active, permit inbound UDP on
that port and UDP 28461 for discovery on the trusted LAN. A multihomed host currently advertises on
its default IPv4 LAN interface; Manual IP is the fallback when the desired interface differs.

In LAN play, `F6` requests the next open seat. The top-left HUD shows connection state and all five
seats: `YOU`, `MAN`, or `AI`. Role claims are host-arbitrated, so two clients cannot own the same
station. In local Stützpunkt, `F6` is the keyboard player's KIA-post takeover control instead.

## Station controls

- **Commander:** mouse/arrows scan; WASD drives only when Driver is empty; LMB covers an empty
  Gunner; C covers empty coax/bow-MG stations; M reads the map.
- **Driver:** WASD drives and steers; Z kills/cranks the Maybach.
- **Gunner:** mouse lays the real-rate turret; LMB fires the 88; C fires coax; RMB changes TZF power;
  Q/Space leaves the optic so a two/three-person gunner can cross to the breech.
- **Loader:** the same embodied ritual as solo: LMB + mouse down opens the lever, E equips the
  looked-at round, align it with the open breech, then LMB + a fast forward shove rams it. Mouse up
  on the lever closes it. The host receives the service intent while the local player keeps the
  physical shell/lever feedback.
- **Radio:** LMB or C fires the bow MG; M reads the paper map.

`Q` from the buttoned cupola drops into the fighting compartment. Look down toward the red bottle
and use E/LMB to take the extinguisher; LMB sprays it. Holding F remains the panic shortcut from any
post. A LAN role determines responsibilities, but no longer locks the player to one camera perch.

Hold X for 1.5 seconds at the open cupola to dismount with the Luger. A dismounted network player no
longer leaves a ghost body occupying an interior post.

M raises a physical sheet and pocket compass in the crewman's hands; there is no permanent compass
HUD. The world keeps moving in solo and LAN. Relative mouse movement positions the pencil and LMB
lays append-only graphite — there is deliberately no eraser.

## Verification

```sh
cargo test --bin kessel
cargo run -- lan-verify
cargo run -- koop /tmp/kessel-koop
```

`lan-verify` opens two real localhost UDP sockets and proves handshake, role assignment, host
mission authority, two-person fallback, gunner input delivery and world-snapshot reconciliation.
The `koop` proof walks the Stützpunkt mission with all four local posts, exercises casualty takeover,
and writes per-seat plus 2×2 split-screen captures.
