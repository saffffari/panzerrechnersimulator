"""Crew voices v2 (2026-11-08): roles, voice designs and the showcase line list.
Wording is ours (plausible Wehrmacht tank-crew intercom German), not taken from a period source,
unless a line's note says otherwise."""

# Role -> (German post, English, Qwen3-TTS VoiceDesign instruct). All five aim for a low speaking
# pitch; urgency is asked for as pace and force with the pitch held down.
ROLES = {
    "KDT": ("Kommandant", "commander",
            "Deep bass-baritone voice of a German man of about 38, a tank commander. Very low pitched, "
            "dark chest resonance, slightly gravelly. Calm authority: short, dry, clipped commands, "
            "firm and level. Native German speaker, no accent, close microphone."),
    "RS": ("Richtschütze", "gunner",
           "Low, dark baritone voice of a German man of about 27, a tank gunner. Quiet, focused and "
           "precise, terse replies, steady and level, very little melody. Native German speaker, no "
           "accent, close microphone."),
    "LS": ("Ladeschütze", "loader",
           "Low-pitched, rough and slightly hoarse voice of a young German man of about 21, a tank "
           "loader. Short, loud, blunt reports, breathy from physical work, but the voice stays low. "
           "Native German speaker, no accent, close microphone."),
    "FA": ("Fahrer", "driver",
           "Very deep, slow, gravelly bass voice of a German man of about 33, a tank driver. Laconic "
           "and matter-of-fact, heavy and unhurried. Native German speaker, no accent, close microphone."),
    "FU": ("Funker", "radio operator",
           "Low, even baritone voice of a German man of about 29, a radio operator. Clear crisp "
           "articulation, measured and businesslike, slightly nasal. Native German speaker, no accent, "
           "close microphone."),
}

# Delivery added to the role's voice. Never ask for shouting high or screaming: urgency = pace + force.
DELIVERY = {
    "calm": " He speaks calmly at a steady moderate pace, level intonation, low in his voice.",
    "urgent": (" He speaks fast, hard and clipped with urgent force, like an order in combat, but his "
               "voice stays deep and low: no rise in pitch, no shouting high, no panic, controlled."),
}

REF_TEXT = ("Kiebitz an alle. Wir stehen am Waldrand, die Sicht nach Osten ist frei. "
            "Munition reicht, der Motor läuft ruhig.")

# id, role, register, German, English
LINES = [
    ("K01", "KDT", "calm",   "Richtschütze! Panzer, zwo Uhr, achthundert, am Waldrand.",
     "Gunner! Tank, two o'clock, eight hundred, at the edge of the woods."),
    ("K02", "KDT", "urgent", "Panzer, drei Uhr, nah! Turm rechts, schnell!",
     "Tank, three o'clock, close! Turret right, quick!"),
    ("K03", "KDT", "urgent", "Feuer!", "Fire!"),
    ("K04", "KDT", "calm",   "Zu kurz. Hundert mehr, gleiches Ziel.",
     "Short. Add one hundred, same target."),
    ("K05", "KDT", "calm",   "Abschuss. Gut gemacht. Nächstes Ziel.",
     "Kill. Well done. Next target."),
    ("R01", "RS",  "calm",   "Erkannt.", "Identified."),
    ("R02", "RS",  "urgent", "Treffer! Er brennt!", "Hit! He's burning!"),
    ("R03", "RS",  "calm",   "Daneben. Richte neu.", "Missed. Re-laying."),
    ("L01", "LS",  "calm",   "Panzergranate geladen!", "AP loaded!"),
    ("L02", "LS",  "urgent", "Feuer im Kampfraum!", "Fire in the fighting compartment!"),
    ("L03", "LS",  "urgent", "Ich bin getroffen! Mein Arm!", "I'm hit! My arm!"),
    ("F01", "FA",  "urgent", "Treffer vorne! Die Platte hält.", "Hit up front! The plate is holding."),
    ("F02", "FA",  "calm",   "Verstanden. Fahre an.", "Understood. Moving off."),
    ("U01", "FU",  "calm",   "Adler von Kiebitz: ein Feindpanzer abgeschossen.",
     "Adler from Kiebitz: one enemy tank knocked out."),
    ("U02", "FU",  "urgent", "Treffer links vorne! Funkgerät ausgefallen!",
     "Hit front left! The radio is out!"),
]
LINE = {l[0]: l for l in LINES}
COMPARE = "K02"   # the comparison reel line (old chain vs new chain)
