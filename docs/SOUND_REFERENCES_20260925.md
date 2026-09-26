# Sound references reviewed 25 September 2026

These sources inform the winter map's audio direction. They do not establish measured Tiger I
sound-pressure levels, spectra, armor performance, or a universal human response to a hit. The
oral histories are retrospective personal accounts from other vehicles and theaters. No recording
from these sources was downloaded, sampled, or added to the game during this research pass.

## Primary testimony

### Abrupt strikes, hot fragments, and loss of orientation

[Gerry Chester, National Army Museum interview NAM.2012-06-4-1 (2012), “Hitler Line” transcript](https://www.nam.ac.uk/transcript-hitler-line).
Chester served in the North Irish Horse from 1941–1946. Recalling combat in Italy, he describes
three hard starboard strikes, an immediate bailout, hot fragments around the turret, and later
penetration and ammunition fire. His short description of the strikes is “bang, bang, bang.”
The account concerns his Churchill crew, not a German Tiger. His observation that the gun fired
after evacuation is part of the recollection, not a firing-mechanism specification.

Design inference: make a hit abrupt and physically local. Separate the initial steel strike from
later fragmentation, internal fire, and loose objects. Do not guarantee a long warning whistle or
an exterior view identifying the attacker. The requested two-second KIA blackout is a deliberate
game presentation choice, not a claim derived from this account.

### Small-arms strikes are a different sound class

[Donald Mercier, Wisconsin Veterans Museum oral history OH66](https://wisvetsmuseum.com/wp-content/uploads/2020/06/Mercier-Donald_OH66.pdf),
PDF page 12, passage beginning with his recollection of sitting in the tank during Pacific fighting.
Mercier compares rifle and machine-gun rounds hitting the tank to hail: “it sounds like hail.”
He also stresses his restricted view through the periscope. The account describes his US tank
experience in the Pacific, not measurements from a Tiger I.

Design inference: individual bullet contacts should be brief, lighter, and clearly distinct from
the body and plate resonance of an anti-tank shell. Use actual collision cadence and local bearing.
Do not play the full cannon-clang identity on each bullet or fabricate many extra impacts around
one measured collision. A burst audition is explicitly staged; live gameplay remains event driven.

### Engines expose a position; not all hits feel the same

[James C. Begay, WPT Korean War Stories / Wisconsin Veterans Museum OH1003 (2004)](https://wisvetsmuseum.com/wp-content/uploads/2017/03/Begay-James-_OH1003-1.pdf),
printed pages 15–17. Begay recalls cold nights, periodically running an auxiliary engine to charge
the battery, and avoiding unnecessary main-engine operation because of its noise. He describes a
sudden near-hatch strike with brief flame and smoke and incapacitated crewmates. A later hit low
around the final drive produced a much smaller felt disturbance. This was his Marine tank-driver
service in Korea; it should not be presented as Eastern Front testimony.

Design inference: engine-off must reveal a quieter environment, while starting, pulling away,
steering, coasting, and traversing remain distinguishable. Impact location and actual cabin
consequences should shape severity. Avoid making every stopped round sound like catastrophic
penetration. The narration is useful for perceptual direction but cannot calibrate medical injury,
acoustic levels, or armor resistance.

No directly applicable primary period description of Eastern Front wind acoustics was established
in this bounded search. The wind treatment remains a field-recording and procedural interpretation.

## Candidate recording material

The individual creator pages below explicitly listed Creative Commons Zero when inspected.
The original-download links require Freesound login. They are candidates, not admitted game assets;
no files, hashes, cuts, or runtime routing exist for them yet. Any eventual import should preserve
original bytes, the exact source/license record, checksums, and the cut/processing recipe.

| Candidate | Creator and original recording provenance | Published format | Possible use and limitation |
|---|---|---|---|
| [Metal Radiator Hit Bang Impact 96kHz](https://freesound.org/people/qubodup/sounds/50674/) | qubodup; radiator struck by creator, Zoom H2, edited in Audacity; posted 31 March 2008; CC0 | 33.318 s, 9.5 MB FLAC, 96 kHz / 24 bit, stereo | Metal contact/ring building blocks. This is a radiator, not an armor or tank recording. |
| [Heavy gusts of wind](https://freesound.org/people/SamsterBirdies/sounds/565140/) | SamsterBirdies; Zoom H1n; posted 29 March 2021; CC0 | 55.062 s, 4.7 MB FLAC, 44.1 kHz / 16 bit, stereo | Gust texture. Creator reports reducing wind clipping in post; audition for remaining distortion before admission. |
| [Cotton clothes rustling](https://freesound.org/people/khenshom/sounds/481075/) | khenshom; Audio-Technica boom microphone and Tascam DR60dM2; posted 20 August 2019; CC0 | 38.452 s, 14.1 MB WAV, 48 kHz / 32 bit, stereo | Quiet sleeves, equipment handling, or posture changes. No claim of period uniform material. |
| [Paper – folding 01](https://freesound.org/people/Anthousai/sounds/398896/) | Anthousai; paper folded by creator, Tascam DR-40; posted 4 August 2017; CC0 | 17.091 s, 9.4 MB WAV, 96 kHz / 24 bit, stereo | Physical map unfold/fold. Use only when the actual prop changes state. |

None is an authentic Maybach engine recording. No music or recorded narration is proposed for
reuse. Continuous tonal tension beds are not needed: preserve space for a musician's later score
and let machinery, wind, cloth, and isolated contact sounds establish the current atmosphere.

## Runtime audit and listening evidence

The existing engine path already receives independent speed, throttle, load, gear, RPM, and
running state. It lowers revs on an upshift, loads steering/pivot demand, and shuts down to silence.
The largest audit defects were bullet contacts using cannon-clang identities and quiet mechanical
recordings losing their requested bearing. The sound-team changes address these separately from
the engine's existing telemetry.

`src/audio_terror_review.rs` creates bounded, explicitly staged WAV fixtures through the same
ordinary `Audio` sink graph as gameplay. Its receipt records source-bank selection, cue times,
peak/RMS/channel energy, and one shared attenuation-only export gain. The KIA fixture requires
exactly 88,200 silent stereo frames at 44.1 kHz during the two-second gate. It deliberately resumes
the shared co-op listener afterward; a fatal singleplayer Game remains muted.

These listening fixtures validate routing and digital sample behavior. They are not microphone
captures of combat, proof of speaker/headphone playback, or objective historical acoustic matches.

The first exported suite is `captures/astra/audio_terror_20260925_v1_*`: weather, drivetrain,
mechanisms, armor, individual MG contact hail, and the KIA cut. The six WAVs total 82 seconds at
44.1 kHz, stereo PCM16. The accompanying JSON preserves the unquantized peak/RMS measurements.
The run passed with no nonfinite samples, a largest sample peak of 0.5052, and export gain 1.0.
Independent inspection of the encoded KIA WAV confirmed every PCM byte from 3.000 to 5.000 seconds
is zero, with signal before the cut and after the explicitly staged co-op resume.

The optional `export_handling_sound_review` ignored test renders a separate five-second companion
through `map_open`, `map_close`, `compass_equip`, and `compass_stow`. These are generated paper/cloth
fallbacks, not the CC0 recordings listed above. The live game and the capture fixture use the same
pan-aware control entry point. Set `KESSEL_AUDIO_HANDLING_PREFIX` to a fresh flat capture prefix;
the test refuses to overwrite an existing WAV or receipt.
