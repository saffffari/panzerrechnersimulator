# From simulator to release — a brainstorm (3 October 2026)

This is a proposal document, not a description of implemented work. Nothing here is promoted to
"implemented" status. It answers three questions: why the game feels slow, how to turn the
simulator into an emotional experience, and what a Steam release at an indie price would need.

## 1. Where we are

The simulation is close to its ceiling. The Tiger drives, lays, loads, fires, burns and breaks in
ways that are individually convincing. That ceiling is also historical: a Tiger traverses at
6 degrees a second, climbs slowly, and must stop to shoot. More fidelity will not make the game
more exciting, and the 3 October playtest said so directly: "it feels really slow, and adding
more Soviets wouldn't solve it. We need something more narrative."

What the session log and the code audit show about that slowness:

- **Two real bugs made it worse than designed.** From a standing start on any slope the gearbox
  never left first gear (2.9 km/h), and flat top speed was 30 km/h instead of 43. Both are fixed.
- **The canyon is long and empty by construction.** The route is 4.9 km. Nothing happens in the
  first 640 m. No tank appears in the first 2.9 km. The director waited three to six minutes
  after every contact before sending anyone, and reinforcements circled 400 m from where they
  spawned instead of coming to the player. The numbers have been raised and the reinforcements now
  patrol toward the Tiger, but that is a patch on a structure, not a structure.
- **There is no reason to be anywhere.** The run has one goal disc at the far end and no
  checkpoint. Between contacts the player has nothing to decide, find, fear or hope for.
- **Dying costs minutes and teaches nothing.** Death is 11.5 seconds of card, a world rebuild,
  and the same empty first kilometre again.

The conclusion is the owner's own: the missing thing is not content density, it is meaning.

## 2. The thing only this game has

Most tank games are about the gun. This one is already about the box: five men in a dark steel
room, seeing the war through slits, hearing it through armour. The project's first principle is
environmental storytelling with no spoken or written words, grounded in real events, as an
anti-war work. That is the release pitch, and it is rare. The closest references are not tank
games at all:

- *Papers, Please* and *Return of the Obra Dinn*: one constrained viewpoint, a strong visual
  identity, and meaning that arrives through procedure.
- *Highfleet* and *Pacific Drive*: a machine you maintain is the main character.
- *This War of Mine* and *Valiant Hearts*: war games sold on feeling, at exactly this price.
- *Iron Lung*: a tiny, dark vehicle interior and a view through a slit, sold for 6 dollars to a
  very large audience. The cabin here is a far richer version of that idea.

The low-resolution, physically lit look is an asset. It reads as a deliberate style in a
screenshot, which is what a Steam capsule needs.

## 3. How to make it an emotional experience

Ordered roughly by how much feeling each buys per unit of work.

### 3.1 Make the crew people, without a single word

Today the crew are operators. They should be the story.

- **Small wordless rituals.** A photograph tucked beside the gunner's sight. A loader who taps the
  shell twice before ramming. A driver who hums. A cigarette passed across the turret after a
  fight. The cigarette mechanic already exists and is liked; make it the game's rest beat.
- **Wounds and absence.** The blood-volume and wound system the owner has asked for fits here.
  When the loader is hit, the player loads. When a seat is empty, it stays empty, with the
  photograph still there. Loss is felt as extra work and a quieter cabin.
- **Fear you can hear.** Breathing, a prayer rhythm, hands shaking on the traverse wheel after a
  near miss. No dialogue, so nothing to translate.

### 3.2 Give the route a spine of places

Replace "drive 4.9 km to a disc" with five to seven places, each a wordless scene with a human
event the player can read from objects, damage and bodies, and each visible from a distance.

- **Landmarks you steer by.** A burned grain elevator, a railway viaduct, a church tower, a
  frozen river bend, a downed aircraft. The owner has asked for exactly this, drawn on a hand
  map. The map then becomes play: the commander plots, the pencil erases.
- **Each place asks one question.** A column of refugees on the only road. A field hospital in a
  barn with a red cross on the roof and a gun dug in beside it. A bridge wired to blow with men
  still on the far side. The player answers with the tank, and the game never comments.
- **Consequences persist down the road.** What the player did at the second place is visible,
  changed, at the fifth.

This also fixes pacing mechanically. Travel between places is two to three minutes, not eight,
and every leg has something on the horizon.

### 3.3 Change what the enemy is for

- **Fewer anonymous duels, more situations.** An ambush in a village street. A night laager where
  the right answer is to creep past with the engine low. A lone anti-tank gun covering a ford,
  whose crew runs when it is flanked. The systems for these exist; they need staging.
- **Let the enemy be people too.** Bailing crews, a tank that stops firing to tow another, a
  soldier who surrenders. The game must never reward a kill with a number.
- **Pressure that comes to the player.** Reinforcements now advance down the corridor. Add a
  pursuing force behind, so stopping has a cost and the slow traverse becomes tension, not waiting.

### 3.4 Make the slowness the point

A Tiger is slow. Do not hide it; aim it.

- **Fill the driving with jobs.** The commander reads the map and scans. Something always needs
  attention: a track pin working loose, fuel, a wounded man, the radio.
- **Give climbs a view.** A hill should end in a reveal of the next landmark or the next threat.
- **Compress dead time honestly.** A short hold-to-doze on a quiet road that fades to the next
  place is acceptable, as long as it is a thing the commander does and it can be interrupted.

### 3.5 Structure and death

- **A run of about 45 to 70 minutes**, broken at each place by an autosave. A full playthrough of
  three acts is then three to four hours, which is right for the price.
- **Death should end a crew, not a session.** The next crew finds the wreck of the last one where
  it burned. That is cheap to build on the existing wreck and campaign-save code and it is the
  kind of detail people post clips of.
- **An ending that is earned and quiet.** The route ends when the fuel does, or the crew does.

### 3.6 Sound as the script

With no words, sound carries the narrative. The adaptive score now holds a piece through its
phrase instead of swapping every shot. Next: one recurring musical figure per crew member,
silence as a deliberate tool after losses, and the engine as the constant that makes its
stopping frightening.

### 3.7 The owner's arc: the road to Berlin (added later on 3 October)

The owner has since set the spine. The game opens in the blizzard with one order: head west and
defend Berlin. Everything after is the journey back, ending at the Reichstag against a force
that cannot be beaten. The tank is destroyed and the player dies. Or the player bails out, runs,
finds a running car, and drives out of the city to a farmhouse on open land. Home. To fight is to
die; to refuse is to live.

This answers two of the open questions below and sharpens the rest of this document:

- **The places of section 3.2 become stations on a retreat.** Each one is further west and worse
  than the last, and the crew's rituals wear down with them.
- **The second ending must be discoverable, not announced.** Every earlier bail-out, every time
  the player chose the hatch over the gun, should quietly teach that leaving the tank is
  possible. The on-foot systems, the spare-tank climb and the cigarette already exist for this.
- **The Enigma machine as the story's voice.** Orders arrive as decoded strips at the radio
  operator's seat. This is the one place written words enter the game, which makes them carry
  weight. Generated text needs a fixed script underneath it: the same orders for everyone, with
  generation only colouring them. A model that invents events would break the grounding in real
  history that the project requires, and a shipped game cannot depend on a live service.
- **The drive home is the reward.** After hours of slits and darkness, the last sequence is an
  open windscreen, daylight and no enemy. The contrast is the ending.

## 4. Price and positioning

**Recommendation: 12.99 USD, with a 10 to 15 percent launch discount.**

- 7.99 signals a small or unfinished game and leaves no room for discounts. Steam's revenue for
  indie titles is driven by sales events, and a 12.99 base can sit at 8 to 9 dollars on sale and
  still be worth it.
- 12.99 is the band of *Iron Lung*'s successors, *Highfleet* on sale, and most narrative
  indies of three to five hours. It is defensible if the game has a beginning, a middle and an
  end and runs well.
- Early Access is tempting but risky for a narrative game: people play the story once. A better
  path is a free demo of the first place, kept up through a Steam Next Fest, then a 1.0 launch.

## 5. What an actual release needs

### Must have

1. **Performance.** The 3 October log shows 12 to 24 fps on an RTX 4090, limited by single-thread
   draw submission and shadow work. A release needs a locked 60 fps on a mid-range card at the
   internal resolution. The cabin-lamp shadow fix is the first step; the sky-visibility atlas and
   the port views are next. This is the largest engineering risk.
2. **A complete arc.** One finished route with its places, a start, and an ending, before any
   second map.
3. **Onboarding without words.** The first five minutes must teach driving, laying, loading and
   firing through the situation. The loader's manual procedure needs an assisted mode.
4. **Settings and input.** Rebinding, sensitivity, controller parity, resolution and window
   options, audio sliders, colour-blind-safe status lamps.
5. **Save and resume**, crash reporting, and a way to send a log. The new session log is a start.
6. **Asset licensing audit.** Every model, sound and music cue needs a recorded licence that
   permits commercial release. The CC0 soldier is fine; the sound library and tank models need
   checking one by one.
7. **Windows build and Steam Deck check.** Most buyers are on Windows. Deck Verified is a real
   sales driver at this price and the low internal resolution suits it.
8. **Content handling.** A clear store description of the anti-war intent, and care with insignia:
   German store rules restrict some symbols in games.

### Should have

- Steam achievements that reward care, not kills.
- A photo mode. The look sells the game and players will market it for free.
- Co-op, which exists, as a headline feature only if it is stable.
- Localisation is nearly free because there is no dialogue: menus and the store page only.

### Store page

- A 45-second trailer cut entirely from inside the tank, with no text until the title.
- Six screenshots: the cabin lit by a single lamp, a slit view of a burning village, the hand
  map, a crewman's photograph, a night road, a wreck in snow.
- One sentence: "Five men, one tank, no way home."

## 6. A suggested order of work

1. Performance to 60 fps in the cabin and at the sights.
2. One landmark-to-landmark leg built as a vertical slice: map, two places, the crew rituals, a
   persistent consequence, the autosave.
3. Playtest that slice with people who have never seen the game, watching for where they stop.
4. Build the remaining places of act one, then the demo.
5. Licensing audit and Windows build in parallel with step 4.
6. Next Fest demo, then decide the launch date from the wishlist count.

## 7. Open questions for the owner

- Is the player one continuous crew, or a succession of crews across the retreat?
- How much of the manual loading procedure is the game, and how much is friction?
- Does the game end in escape, or does it never allow one?
