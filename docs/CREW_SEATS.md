# Kessel — Crew, Seats & Role-Switching (design)

Status: **design, not yet built.** Extends the existing `UKesselCrewComponent` capability gating
(`CanMove/CanSteer/CanFire`, module KO → lose capability) with a *who-is-doing-the-job* layer.
Locks the control consequence of crew loss. Last updated 2026-06-30.

## Historical grounding — Tiger I crew of 5
1. **Commander** — turret, raised; commands, observes (cupola), can lay the gun in a pinch.
2. **Gunner** — turret left of breech; lays main gun + **coaxial MG**, uses the TZF sight.
3. **Loader** — turret right of breech; loads main gun, manages ready ammo.
4. **Driver** — hull front-left; drives, steers.
5. **Radio operator / hull gunner** — hull front-right; works the radio + the **hull/bow MG**.

So there are **two machine guns**: coaxial (gunner) and hull/bow (radio operator). Crew = **5**.

## The core mechanic — commander hub + forced relocation
- **Full living crew:** you sit in the **commander's seat** and run the whole tank from there —
  drive, lay + fire the gun, use the gunner's sights — because each living crewman *executes your
  command*. The commander seat is the only one with the full overview.
- **A crewman dies → his role goes dark.** You can only restore it by **physically relocating into
  his seat** and doing the job yourself — and **you can occupy only one seat at a time.**
  - Driver dead → to move, switch to the **driver's seat** (and you can't fight while there).
  - Gunner dead → to fire, switch to the **gunner's seat** (and you can't drive while there).
- **Lose driver AND gunner:** you shuttle between driving and shooting, never both at once →
  *big trouble.* This is the intended tension — **crew are highly valuable, irreplaceable** (ties to
  the permadeath/attrition identity).

## The "can anyone do any job?" question (player vs NPC)
Proposed rule, balancing history vs. playability:
- **The player (commander) is the everyman** — when forced, you may occupy *any* seat and perform
  that role (commanders were the most cross-trained; we extend this to all roles for the player so
  you're never hard-locked). One seat at a time.
- **Living NPC crew do ONLY their own job** — the loader does not become a driver. This keeps each
  specific crewman valuable and makes a specific loss bite in a specific way.
- Net: you degrade from "command everything from one seat" → "manually time-share the dead men's
  jobs from their seats" → eventually a lone operator doing one thing at a time.

## Open questions (flag before building)
- **Switch cost:** instant, or a short relocation delay (~1–2 s, scrambling across the compartment)?
  Recommend a delay so crew loss has weight. (Could scale with how cramped/which seats.)
- **Loader role:** with the loader dead and you NOT in the loader seat, reload already jumps
  (`ReloadMultiplier`). Do we also let the player occupy the loader seat to reload at normal speed,
  or keep loader as a pure penalty (you can lay/fire but reload stays slow)? Recommend: penalty only
  — you can't be gunner and loader at once anyway.
- **Hull MG / radio operator:** model the bow MG as a 5th seat, or fold radio-op loss into a
  comms/spotting penalty and treat the hull MG as a minor secondary? Recommend: fold for M1, expand
  later.
- **Commander absence:** while you're down in the driver/gunner seat, you lose the cupola overview
  (no free-look / spotting). Intended — buttoning down into a role blinds you. Worth a UI cue.

## Build phasing (when we start)
1. Define a `Seat` enum (Commander, Driver, Gunner [, HullGunner/Loader]) + a current-seat state on
   the pawn; bind a "switch seat" control.
2. Gate input by seat × crew-alive: from Commander seat, drive/fire route through living crew; if a
   crewman is dead, that input is inert unless the player is in that seat.
3. Fold into the existing view system — seats own their camera/optic (driver vision slit, gunner
   TZF, commander cupola). The current EViewMode (GunnerOptic/CommanderButtoned/Unbuttoned) becomes
   the commander-seat view set.
4. Switch cost + UI cue. Then hull MG / radio-op expansion.

See [[tank-sim-crew]] (mortal irreplaceable crew) and `CONTROLS.md` (binding layout).
