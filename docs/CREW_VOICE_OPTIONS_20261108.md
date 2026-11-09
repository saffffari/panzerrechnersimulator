# Crew voices: options for deep German intercom voices (8 November 2026)

Written by the background agent for Alex's ask: the radio chatter needs more bass; voices were
"cracking and going high pitch ... like alvin and chipmunks"; target the feel of the crew in
*Gunner, HEAT, PC!* (calm, deep, clipped professional intercom; urgency from pace and force, not
from pitch). Are there paid options or online tools?

Nothing was signed up for and no text or audio was sent to any paid or online service. Prices
and terms below are from the vendors' public pages on 8 November 2026 where a link says so, or
from secondary pages where marked; anything not checked is marked **unverified**.

## What was wrong with the first tests, measured

- **The filter took the bass out.** The first intercom chain was a 6th-order band-pass at
  420-2200 Hz (radio 300-2700 Hz). Energy below 300 Hz after it: **0.0 %** of the voice (the
  same line through the new chain: 34 %). That is the "no base".
- **The squeaks were real, and partly the engines'.** The panic test asked Qwen3-TTS for
  screaming, which put its median pitch at 395-424 Hz (a deep male speaks at 85-130 Hz); its
  "pushed" variant then added +2 semitones of pitch shift. Measuring every new take also caught
  Chatterbox's own defect: some takes jump to a held 400-445 Hz inside a word (4 of 45 takes,
  e.g. "Panzer. 3 Uhr. Nah..." held 444 Hz for over 50 ms; Qwen asked for urgency did it in 10
  of 45). Those are now rejected automatically.
- **Fix in the new pipeline** (below): no pitch shift anywhere, deep voice designs, urgency asked
  for as pace and force, every take measured and the cracked ones thrown out, and a chain that
  keeps 120-300 Hz.

## Options

Costs are for "a few thousand lines": about 3,000 lines of about 35 characters = ~105,000
characters per pass (three takes each to choose from: ~315,000).

| Option | German, deep male, emotion | Can pitch be held down? | Commercial licence | Rough cost | Game time or pre-rendered |
|---|---|---|---|---|---|
| **ElevenLabs** (Eleven v3, Multilingual v2; Voice Design) | German is among v3's best languages per secondary reviews; emotion by inline audio tags ([urgent], [shouts]); Voice Design makes a voice from a description ("deep, gravelly German man, 40") | No pitch parameter; held by choosing or designing a deep voice and steadier stability settings (unverified how far tags move pitch) | Paid plans include commercial use; output made while subscribed stays usable after cancelling (help centre) | 1 credit per character (Multilingual v2); Creator $22/month = 121k credits (one pass), Pro $99 = 600k (three takes) | Pre-rendered (cloud API; per-request latency, no offline) |
| **Cartesia** (Sonic) | German not confirmed on the pages read (unverified); emotion tags **English only** | No pitch control documented (speed 0.6-1.5, volume, emotion) | Paid plans include commercial use | 1 credit per character; Pro $5 = 100k, Startup $49 = 1.25M | Pre-rendered (realtime API exists) |
| **OpenAI** gpt-4o-mini-tts | Speaks German; voice set is fixed (no German-native voices; accent risk, unverified); style by a text instruction ("calm, clipped, low") | Only by instruction; not guaranteed | Usable commercially; OpenAI's usage policy asks you to disclose to listeners that the voice is AI (from memory, **unverified today**) | $0.60/M text in + $12/M audio out, about 1.5 cents per minute (secondary): a pass is a few dollars | Pre-rendered |
| **Azure Speech** (Microsoft) | de-DE male voices Conrad, Bernd, Christoph, Kasper, Killian, Klaus, Ralf, Florian (multilingual, HD); styles on Conrad (cheerful, sad) and on the new Klaus MAI-Voice-2 (angry, determined, fearful, shouting, ...) | **Yes, explicitly**: SSML `<prosody pitch="-10%" range="x-low">` sets the baseline and narrows the range (0.5-1.5x); whether it applies to the HD MAI voices is unverified | Standard Azure terms; commercial use allowed | About $15-16 per million characters for standard neural voices (secondary; HD ~$22); 0.5M characters a month free (F0) | Pre-rendered (also has an on-device "embedded speech" SDK, unverified for de-DE) |
| **Google Cloud TTS** (Chirp 3: HD) | de-DE supported; natural but no emotion tags | Pitch only through SSML `<prosody>` in synchronous requests; speaking rate 0.25-2.0 | Standard Google Cloud terms | $30 per million characters, first 1M a month free: a pass is free | Pre-rendered |
| **Hume** (Octave 2) | German listed for Octave 2 (preview) on secondary pages | Emotion by description | Free and Starter tiers non-commercial (FAQ, secondary) | Unverified | **Avoid**: the TTS docs pages now show only an "Audio Decisions API", and a secondary source says the TTS API ends 13 Nov 2026 (unverified) |
| **PlayHT** | - | - | - | - | **Gone**: shut down after Meta hired the team (API stopped mid-2025, platform closed 31 Dec 2025, secondary sources) |
| **Resemble AI** (hosted) | - | - | - | Was $0.0005 per second | **Avoid**: a review of Oct 2026 says it no longer sells voice generation to new customers (unverified). Its open model Chatterbox is the local option below |
| **Fish Audio** (S1, API) | Multilingual incl. German (unverified quality) | By reference voice | API is pay as you go; open weights need a paid commercial licence | $15 per million UTF-8 bytes | Pre-rendered |
| **Inworld** (TTS-1.5) | German listed | Unverified | Unverified | $5-10 per million characters (vendor page, tiers vary) | Realtime API (game NPC focus) |
| **Local: Qwen3-TTS 1.7B VoiceDesign** (installed) | German; a voice from a text description; emotion by description | By description only; it drifted up to 200-260 Hz on "loud" urgent lines even when told not to | Apache-2.0 | Free (RTX 4090: ~5 s per line) | Pre-rendered (GPU) |
| **Local: Chatterbox Multilingual** (installed) | German; clones a reference voice (one voice per crewman); exaggeration and pacing knobs | **Yes in practice**: it keeps the reference's pitch (measured medians 76-129 Hz this run); occasional squeak takes, caught by the measurement | MIT (output carries an inaudible watermark) | Free (~1 s per line) | Pre-rendered; perhaps at game time on the GPU, but it competes with rendering |
| **Local: Zonos v0.1** (installed) | German; cloning + emotion vector + `pitch_std` | `pitch_std` narrows the pitch range | Apache-2.0 | Free | Pre-rendered |
| **Local: Piper** (installed; de_DE thorsten, karlsson, mls) | German, flat delivery, deep MLS speakers (80-95 Hz) | Fixed per voice | Per voice: thorsten CC0; MLS CC BY 4.0; **pavoque non-commercial: do not use** | Free, CPU | **Game time**: real-time on CPU |

Sources: [ElevenLabs pricing](https://elevenlabs.io/pricing),
[ElevenLabs: can I publish](https://elevenlabs.io/docs/help-center/legal/can-i-publish-the-content-i-generate-on-the-platform.md),
[Eleven v3 audio tags](https://help.elevenlabs.io/hc/en-us/articles/35869142561297-How-do-audio-tags-work-with-Eleven-v3),
[German as a strong v3 language (secondary)](https://www.createwith.com/tool/elevenlabs/updates/elevenlabs-makes-eleven-v3-conversational-generally-available-for-realtime-voice),
[Cartesia pricing](https://cartesia.ai/pricing),
[Cartesia speed/volume/emotion](https://docs.cartesia.ai/build-with-cartesia/sonic-3/volume-speed-emotion),
[OpenAI API pricing](https://developers.openai.com/api/docs/pricing),
[gpt-4o-mini-tts per-minute estimate (secondary)](https://blog.promptlayer.com/gpt-4o-mini-tts-steerable-low-cost-speech-via-simple-apis/),
[Azure voices and styles](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/language-support?tabs=tts),
[Azure SSML prosody pitch and range](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/speech-synthesis-markup-voice),
[Azure price (secondary)](https://texttolab.com/blog/azure-text-to-speech-pricing),
[Google TTS pricing](https://cloud.google.com/text-to-speech/pricing),
[Chirp 3: HD controls](https://docs.cloud.google.com/text-to-speech/docs/chirp3-hd),
[Hume TTS overview](https://dev.hume.ai/docs/text-to-speech-tts/overview),
[Hume shutdown claim (secondary, unverified)](https://www.aiwiki.ai/wiki/hume_octave_2),
[PlayHT shutdown (secondary)](https://texttolab.com/blog/play-ht-shutdown-alternatives),
[Resemble availability (secondary)](https://www.voiceflow.com/articles/resemble-ai),
[Fish Audio API pricing](https://docs.fish.audio/developer-guide/models-pricing/pricing-and-rate-limits),
[Inworld TTS](https://inworld.ai/tts),
Qwen3-TTS, Zonos, Chatterbox licences from their model cards (earlier test's README,
`/home/alex/panzer/captures/tts-panic-20261108-README.txt`).

## What was made locally (the refined set)

Pipeline (`/home/alex/panzer/tts-radio-work-20261108/crew-v2/scripts/`, copies of the chain
and the callout planner in `tools/voice/`):

1. **A voice per crewman designed once** with Qwen3-TTS VoiceDesign from a description (deep
   bass-baritone commander of about 38, low dark baritone gunner, rough low loader, very deep slow
   driver, even baritone radio operator), four reads of a neutral sentence each; the read whose
   median pitch lay in range was kept as that man's reference (commander 126 Hz, gunner 90,
   loader 111, driver 93, radio operator 124).
2. **Every line cloned from its man's reference** with Chatterbox Multilingual, three takes each,
   low exaggeration (0.35 calm, 0.5 urgent); urgency through a lower `cfg_weight` (faster, harder
   pacing) and the wording. **No pitch shifting at all.**
3. **Every take measured and filtered**: pYIN pitch over speech frames only (median, 95th
   percentile, highest pitch held 50 ms), share of energy below 300 Hz and in 120-300 Hz, and a
   Whisper large-v3-turbo transcription against the text. Rejected: median outside 75-140 Hz, 95th
   percentile or held pitch above 200 Hz, no measurable pitch, character error over 25 %. 9 of 45
   Chatterbox takes and 28 of 45 direct Qwen takes failed; every line kept a Chatterbox take.
4. **The new intercom chain** (throat microphone, intercom amplifier, headset): high-pass 85 Hz,
   low-pass 4.2 kHz (two poles twice), +3 dB at 170 Hz, +2.5 dB at 650 Hz, -3 dB at 2.7 kHz, a
   compressor, the amplifier driven into tanh saturation, a fast limiting compressor, and a light
   bed of pink hiss and the engine's 200 Hz firing tone. A radio (net) variant is narrower (140 Hz
   - 3.4 kHz) and driven harder, with carrier hiss and key clicks while keyed and no squelch tail.

Files in `/home/alex/panzer/captures/`: `tts-crew-20261108-v2-<line>-<post>-dry.wav` and
`...-intercom.wav` for the fifteen showcase lines, and
`tts-crew-20261108-v2-comparison-K02-dry-old-new.wav` (one line dry, then through the old intercom
chain, then the new one). Measurements: `/home/alex/panzer/tts-radio-work-20261108/crew-v2/out/table.txt`
and `selection.json` beside it. Pitch medians of the kept takes: commander 122-129 Hz, gunner
76-91, loader 93-115, driver 81-84, radio operator 117-118; no kept take has a 95th percentile
above 157 Hz. The panic test's lines measured 232-424 Hz.

## The voices in context

`/home/alex/panzer/captures/demo-20261101-crew-voices-context-20261108.mp4` (120 s, 5120x2880, the
1024x576 grid x5 nearest, 4:4:4 HEVC): `ai-film` act 1, three neural T-34s on a 220 m ring, seed
29, filmed with `KESSEL_AI_FILM_INTERIOR=1` (the turret-rear lens throughout, turned on the hole
when the Tiger is holed) and `KESSEL_AI_FILM_NO_CAPTION=1`, the locked look
(`tools/voice/locked_env.sh`), the game's own audio. The receipt's crew-callout events
(`src/game_ai_film.rs`) were turned into lines by `tools/voice/crew_callouts.py`, spoken by the
same five voices (`tools/voice/film_select.py`), placed on the film's clock (26 of 47 candidate
lines; the rest gave way to more important ones), the game sound ducked 5 dB under each, and the
German line with the English under it burned in (`tools/voice/compose_context_film.py`). The
commander's "Feuer!" is placed to end just before each logged shot: the order is inferred from
the shot, the log has no separate order event. Nobody has listened to them yet:
the agent cannot hear, and every judgement above is a measurement.

## Recommendation

**Keep the local pipeline for the playtest trial** (Qwen3-TTS to design one deep voice per man,
Chatterbox to speak every line in it, the measurement gate, the new chain): it is free, its
licences are clean for a commercial game (Apache-2.0 and MIT), it already meets the pitch target
on every line, and lines for new events can be rendered in a minute. If Alex's ear says the
delivery is still not *Gunner, HEAT, PC!*, the paid step is **ElevenLabs Eleven v3** with five
designed deep German voices, pre-rendered, about $22-99 for the whole set on one month's plan;
`tools/voice/render_elevenlabs.py` renders the line list from an `ELEVENLABS_API_KEY` in the
environment (**untested**: no account or key was used). **Fallback: Azure Speech** standard de-DE
voices (Conrad, Klaus, Ralf), the one service that lets you pin the pitch down in markup
(`prosody pitch` and `range`), for a few dollars or inside the free tier.
