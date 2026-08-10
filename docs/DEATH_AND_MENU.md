# Kessel — Death, Penetration Feedback & Front-End (design spec)

Status: **design, not yet built** (set 2026-06-30). All on-screen text is **GERMAN only — no English**.
Needs two new subsystems: a **text/UI renderer** (engine currently draws only the reticle) and a
**game-state machine** (Menu ↔ Playing ↔ Death). Ties to the armor/penetration model (front bounces,
flank/rear penetrate) and the kill-drama/damage-viz work.

## Penetration feedback (so a hit is never silent)
The complaint being fixed: "the gun just stops and you don't know what happened." Every penetration
must give a clear cause.

- **Penetration, spall does NOT kill the player** → an **interior explosion flash** inside the tank
  view (bright blast + concussion + smoke/sparks in the compartment). You survive but *know* you
  were holed — a crew member may be knocked out / a module disabled (→ crew-of-one degradation,
  fire, repair). This is the "you got penetrated and lived" feedback.
- **Ammunition detonation, OR a round striking the player/commander directly** → **NO explosion
  drama. Instant black screen.** You're simply dead — abrupt, merciless. (Catastrophic-kill path
  from the armor model.)

## Death sequence
1. Screen goes **black** (instant on catastrophic; after the interior-blast beat on a lethal spall).
2. After ~2 s, big bold centered German text: **`Du bist gestorben`**
3. Below it, a randomly selected fallen **Panzer ace** as a memorial epitaph, German memorial
   notation (`*` = born / `†` = died), death year stylized to **1942** (the Kessel):
   ```
   {name}
   * {birthdate, German format}   † 1942
   ```
   e.g. `Michael Wittmann` / `* 22. April 1914   † 1942`
4. Hold a few seconds → **fade to black** → return to the **main menu**.

### Panzer aces list (real names + real birthdays; death rendered as 1942)
German date format (`Tag. Monat Jahr`). Pick one at random per death.
- Michael Wittmann — * 22. April 1914
- Kurt Knispel — * 20. September 1921
- Otto Carius — * 27. Mai 1922
- Ernst Barkmann — * 25. August 1919
- Johannes Bölter — * 19. Februar 1915
- Rudolf von Ribbentrop — * 11. Mai 1921
- Albert Ernst — * 8. November 1912
- Paul Egger — * 4. April 1916

(Birthdays are the real historical dates; the 1942 death year is the game's stylization — several
of these men survived the war. Extend the list freely; keep dates accurate.)

## Main menu — "der Kessel"
- Massive **`der Kessel`** title logo, dominant.
- Modes listed below it (German), e.g.: **`Kampagne`** (campaign), **`Gefecht`** (skirmish/battle),
  **`Überleben`** (survival) — final mode set TBD.
- **Time-of-day mode selection: `Tag` (day) / `Nacht` (night)** — the player picks the setting from
  the title page. `Tag` = the current overcast cold-blue day. `Nacht` = the dusk/night residential
  Stalingrad look (glowing windows, snowfall, laser tracers — see the night-look art direction).
  Store the chosen `TimeOfDay { Tag, Nacht }`; the game reads it to build the day vs night scene
  (scene-side wiring lands with the night-look pass).
- **`Einstellungen`** (settings)
- **`Beenden`** (quit)
- Everything German. Cold, stark, wintry presentation consistent with the ray-traced-PS1 look.

## Build notes
- **Text renderer:** a simple approach fits the PS1 aesthetic — a bitmap/atlas font drawn as
  textured quads at display res (or a chunky low-res font pre-crush). Needs German glyphs incl.
  `ö ü ä ß * †`. No need for a heavyweight font stack.
- **State machine:** `AppState { Menu, Playing, Death { t, ace } }` in main.rs; the current
  boot-straight-into-play becomes `Menu → Playing`. Death is entered from the game when
  `player.alive` goes false, carrying the death type (catastrophic = instant black; spall = after
  the interior-blast beat).
- German UI strings table lives with the front-end code.
