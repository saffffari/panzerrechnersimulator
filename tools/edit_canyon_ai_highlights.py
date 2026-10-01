#!/usr/bin/env python3
"""Edit captured Canyon attempts, with review-only captions from their telemetry."""
import json
import math
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
CAP = ROOT / "captures"
OUT = CAP / "Panzerrechnersystem_Canyon_AI_Highlights_20260930.mp4"
SEEDS = (73, 101)


def stamp(seconds):
    centis = round(seconds * 100)
    return f"{centis // 360000}:{centis // 6000 % 60:02}:{centis // 100 % 60:02}.{centis % 100:02}"


def clock(seconds):
    return f"{int(seconds) // 60:02}:{int(seconds) % 60:02}"


def probe(path):
    return json.loads(subprocess.check_output([
        "ffprobe", "-v", "error", "-show_streams", "-show_format", "-of", "json", str(path)
    ]))


def main():
    events = []

    def title(start, end, style, text):
        text = text.replace("\n", r"\N")
        events.append(f"Dialogue: 0,{stamp(start)},{stamp(end)},{style},,0,0,0,,{text}")

    offset = 0.0
    fatal_local = None
    films = []
    inputs = []
    receipts = []
    for seed in SEEDS:
        prefix = CAP / f"canyon_ai_film{seed}_20260930"
        path = prefix.with_suffix(".mp4")
        metadata = probe(path)
        video = next(s for s in metadata["streams"] if s["codec_type"] == "video")
        duration = float(video["duration"])
        segments = json.loads(Path(f"{prefix}_segments.json").read_text())["segments"]
        trace = [json.loads(line) for line in Path(f"{prefix}_trace.jsonl").read_text().splitlines()]
        final = json.loads(Path(f"{prefix}_result.json").read_text())
        expected = json.loads((CAP / f"canyon_ai_attempt{seed}_20260930_result.json").read_text())
        # Visual/audio rendering must not change the outcome being represented.
        assert final["outcome"] == expected["outcome"]
        assert final["final"] == expected["final"], f"seed {seed}: probe/film simulation diverged"
        receipts.append(final)
        films.append(dict(seed=seed, file=str(path), start_s=offset, seconds=duration))
        inputs.extend(["-i", str(path)])
        for i, segment in enumerate(segments):
            begin = segment["film_start_s"]
            end = segments[i + 1]["film_start_s"] if i + 1 < len(segments) else duration
            at = segment["simulation_start_s"]
            label = segment["label"].upper()
            title(offset + begin, offset + min(begin + 4.0, end), "Chapter", label)
            for second in range(math.ceil(end - begin)):
                simulation = at + second
                state = min(trace, key=lambda s: abs(s["time_s"] - simulation))
                caption = f"SEED {seed}     {clock(simulation)}     ROUTE {state['progress']:.0%}     T-34 KILLS {state['kills']}"
                title(offset + begin + second, offset + min(begin + second + 1, end), "Telemetry", caption)
        if seed == 101:
            fatal_local = final["final"]["time_s"] - segments[0]["simulation_start_s"]
            fatal = offset + fatal_local
            title(fatal, offset + duration, "Fatal", "AMMUNITION DETONATION — RUN ENDED")
        offset += duration

    title(0, 5.5, "Heading", "PANZERRECHNERSYSTEM\nAI TIGER / CANYON RUN")
    title(0, 5.5, "Subheading", "Three seeded attempts • First-person game capture")
    # The final 8 seconds are an editorial results card over black, not an in-game interface.
    title(offset + 0.6, offset + 8, "ResultHeading", "NO CLEAR IN THREE ATTEMPTS")
    lines = []
    outcomes = []
    for seed in (73, 29, 101):
        result = json.loads((CAP / f"canyon_ai_attempt{seed}_20260930_result.json").read_text())
        state = result["final"]
        reason = "ammunition detonation" if result["outcome"] == "knocked_out" else "navigation stalled"
        lines.append(f"Seed {seed}     {state['progress']:.0%}     {state['kills']} T-34 kills     {clock(state['time_s'])}     {reason}")
        outcomes.append(result)
    title(offset + 0.6, offset + 8, "Results", "\n\n".join(lines))
    title(offset + 0.6, offset + 8, "Disclosure", "Rule-based test crew • Known route and geometry-assisted sight / aiming\nNormal damage, adaptive reinforcements and physical reloads • No automatic repairs")
    ass = CAP / "canyon_ai_highlights_20260930.ass"
    ass.write_text("""[Script Info]
ScriptType: v4.00+
PlayResX: 1920
PlayResY: 1080
WrapStyle: 2

[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: Heading,DejaVu Sans,48,&H00EAE8E2,&H00FFFFFF,&HAA10100E,&H990B0D0D,1,0,0,0,100,100,1,0,1,2,1,7,68,68,72,1
Style: Subheading,DejaVu Sans,25,&H00D8D8CD,&H00FFFFFF,&HAA10100E,&H990B0D0D,0,0,0,0,100,100,0,0,1,2,1,7,70,68,214,1
Style: Chapter,DejaVu Sans,29,&H00EAE8E2,&H00FFFFFF,&HAA10100E,&H990B0D0D,1,0,0,0,100,100,0,0,1,2,1,9,68,68,72,1
Style: Telemetry,DejaVu Sans,25,&H00EAE8E2,&H00FFFFFF,&HAA10100E,&H990B0D0D,0,0,0,0,100,100,0,0,3,12,0,1,68,68,56,1
Style: Fatal,DejaVu Sans,40,&H0084A4ED,&H00FFFFFF,&HAA10100E,&H990B0D0D,1,0,0,0,100,100,0,0,1,2,1,8,68,68,145,1
Style: ResultHeading,DejaVu Sans,49,&H00EAE8E2,&H00FFFFFF,&H0010100E,&H000B0D0D,1,0,0,0,100,100,1,0,1,0,0,8,68,68,224,1
Style: Results,DejaVu Sans,32,&H00D8D8CD,&H00FFFFFF,&H0010100E,&H000B0D0D,0,0,0,0,100,100,0,0,1,0,0,5,68,68,0,1
Style: Disclosure,DejaVu Sans,25,&H00AAAFA8,&H00FFFFFF,&H0010100E,&H000B0D0D,0,0,0,0,100,100,0,0,1,0,0,2,68,68,170,1

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
""" + "\n".join(events) + "\n")
    assert fatal_local is not None
    filters = (f"[1:v:0]fade=t=out:st={fatal_local:.6f}:d=1.5[vfatal];"
               f"[0:v:0][0:a:0][vfatal][1:a:0]concat=n=2:v=1:a=1[v][a];"
               f"[v]tpad=stop_mode=clone:stop_duration=8,fade=t=out:st={offset:.6f}:d=0.5,ass={ass}[vo];"
               f"[a]apad=pad_dur=8[ao]")
    command = ["ffmpeg", "-v", "warning", "-nostdin", "-y", *inputs, "-filter_complex", filters,
               "-map", "[vo]", "-map", "[ao]", "-t", str(offset + 8), "-r", "30",
               "-c:v", "libx264", "-preset", "fast", "-crf", "18", "-profile:v", "high", "-level:v", "4.1",
               "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "192k", "-movflags", "+faststart", str(OUT)]
    subprocess.run(command, check=True)
    Path(str(OUT.with_suffix("")) + "_edit.json").write_text(json.dumps(dict(
        output=str(OUT), films=films, duration_s=offset + 8, outcomes=outcomes,
        probe_and_recording_final_states_identical=True,
        editing="Chronological excerpts per attempt; editorial telemetry and result captions; native game audio; frozen fatal scene with a 1.5-second editorial fade aligned with the native terminal audio; final black results card.",
        command=command), indent=2) + "\n")
    print(OUT)


if __name__ == "__main__":
    main()
