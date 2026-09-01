#!/usr/bin/env python3
"""Turn Kessel's deterministic WAV audition and cue CSV into a labeled review video.

The picture is deliberately functional: the currently audible cue, its exact runtime source,
timing, and the next three cues remain readable while a stereo waveform provides a visual sync
witness.  The source WAV is passed through unchanged until AAC encoding; no mastering or extra
sound design happens here.
"""

from __future__ import annotations

import argparse
import csv
import json
import math
import shutil
import subprocess
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class Cue:
    index: int
    start: float
    end: float
    category: str
    name: str
    source: str
    master_peak: float


CATEGORY_COLORS = {
    "weapons": "&H00468CF2",
    "impacts": "&H003E62E8",
    "fire-cookoff": "&H002D4FC9",
    "engine": "&H006A9AD0",
    "turret": "&H0084B6DE",
    "fragments": "&H005D72EE",
    "interaction-crew": "&H007A9BC4",
    "ambience": "&H00B8A58A",
}


def parse_cues(path: Path) -> list[Cue]:
    with path.open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))
    cues = [
        Cue(
            index=int(row["index"]),
            start=float(row["start_seconds"]),
            end=float(row["end_seconds"]),
            category=row["category"],
            name=row["name"],
            source=row["source_file"],
            master_peak=float(row["master_peak"]),
        )
        for row in rows
    ]
    if not cues or [cue.index for cue in cues] != list(range(1, len(cues) + 1)):
        raise ValueError("cue CSV must contain contiguous one-based indices")
    if any(cue.start < 0.0 or cue.end <= cue.start for cue in cues):
        raise ValueError("cue CSV contains invalid start/end timing")
    if any(left.end > right.start for left, right in zip(cues, cues[1:])):
        raise ValueError("cue CSV contains overlapping cues")
    return cues


def ass_time(seconds: float) -> str:
    centiseconds = max(0, round(seconds * 100.0))
    hours, centiseconds = divmod(centiseconds, 360_000)
    minutes, centiseconds = divmod(centiseconds, 6_000)
    whole_seconds, centiseconds = divmod(centiseconds, 100)
    return f"{hours}:{minutes:02d}:{whole_seconds:02d}.{centiseconds:02d}"


def clock(seconds: float) -> str:
    millis = max(0, round(seconds * 1000.0))
    minutes, millis = divmod(millis, 60_000)
    whole_seconds, millis = divmod(millis, 1000)
    return f"{minutes:02d}:{whole_seconds:02d}.{millis:03d}"


def ass_escape(text: str) -> str:
    return (
        text.replace("\\", "／")
        .replace("{", "(")
        .replace("}", ")")
        .replace("\n", r"\N")
    )


def cue_display_name(name: str) -> str:
    return name.replace("_", " ").upper()


def cue_status(cue: Cue) -> str:
    """Describe runtime reachability without pretending every offline variant is wired."""
    if cue.name == "live_mg_gravel_right" or cue.name == "fragment_contact_brass_case":
        return "DSP ONLY / NOT WIRED"
    if cue.name.startswith("live_"):
        return "OFFLINE DEMO COMPOSITE"
    if cue.name.startswith("runtime_body_"):
        return "LIVE PARAMETER VARIANT"
    if cue.name.startswith("runtime_"):
        return "LIVE IDENTITY"
    if cue.name.startswith("destruction_dry_"):
        return "LIVE DRY SOURCE"
    return "LIVE PARAMETER VARIANT"


def write_ass(path: Path, cues: list[Cue], duration: float) -> None:
    header = """[Script Info]
Title: Kessel labeled game-audio audition
ScriptType: v4.00+
PlayResX: 1920
PlayResY: 1080
WrapStyle: 2
ScaledBorderAndShadow: yes

[V4+ Styles]
Format: Name,Fontname,Fontsize,PrimaryColour,SecondaryColour,OutlineColour,BackColour,Bold,Italic,Underline,StrikeOut,ScaleX,ScaleY,Spacing,Angle,BorderStyle,Outline,Shadow,Alignment,MarginL,MarginR,MarginV,Encoding
Style: Header,VT323,43,&H00ECE8DF,&H00ECE8DF,&H00000000,&H00000000,0,0,0,0,100,100,4,0,1,0,0,7,84,84,70,1
Style: Counter,VT323,30,&H009B9489,&H009B9489,&H00000000,&H00000000,0,0,0,0,100,100,2,0,1,0,0,9,84,84,76,1
Style: Category,VT323,35,&H00468CF2,&H00468CF2,&H00000000,&H00000000,0,0,0,0,100,100,3,0,1,0,0,7,84,84,202,1
Style: Cue,VT323,68,&H00FFFFFF,&H00FFFFFF,&H00000000,&H00000000,-1,0,0,0,100,100,1,0,1,1.5,0,7,84,84,260,1
Style: Meta,VT323,29,&H00C7C0B5,&H00C7C0B5,&H00000000,&H00000000,0,0,0,0,100,100,1,0,1,0,0,7,86,86,455,1
Style: NextHead,VT323,25,&H007D776F,&H007D776F,&H00000000,&H00000000,0,0,0,0,100,100,2,0,1,0,0,7,86,86,555,1
Style: Next,VT323,27,&H00AAA49B,&H00AAA49B,&H00000000,&H00000000,0,0,0,0,100,100,1,0,1,0,0,7,86,86,598,1
Style: Footer,VT323,25,&H008B867F,&H008B867F,&H00000000,&H00000000,0,0,0,0,100,100,1,0,1,0,0,2,84,84,42,1

[Events]
Format: Layer,Start,End,Style,Name,MarginL,MarginR,MarginV,Effect,Text
"""
    events: list[str] = []
    full_start = ass_time(0.0)
    full_end = ass_time(duration)
    events.append(
        f"Dialogue: 0,{full_start},{full_end},Header,,0,0,0,,K E S S E L  //  GAME AUDIO AUDITION"
    )
    events.append(
        f"Dialogue: 0,{full_start},{full_end},Footer,,0,0,0,,HEADPHONES AT A COMFORTABLE LEVEL  •  AUDITION-NORMALIZED TO −6 dBFS  •  NOT IN-GAME RELATIVE LEVEL"
    )
    for offset, cue in enumerate(cues):
        start = ass_time(cue.start)
        hold_end = cues[offset + 1].start if offset + 1 < len(cues) else duration
        end = ass_time(max(cue.end, hold_end))
        color = CATEGORY_COLORS.get(cue.category, "&H00468CF2")
        category = cue.category.replace("-", " / ").upper()
        status = cue_status(cue)
        next_lines = []
        for upcoming in cues[offset + 1 : offset + 4]:
            next_lines.append(
                f"{upcoming.index:02d}   {cue_display_name(upcoming.name)}"
            )
        if not next_lines:
            next_lines.append("END OF AUDITION")
        meta = (
            f"SOURCE  {cue.source}"
            f"\n{clock(cue.start)} — {clock(cue.end)}   •   {cue.end - cue.start:.3f} S"
            f"   •   CURRENT CUE PEAK {20.0 * math.log10(cue.master_peak):.1f} dBFS"
        )
        events.extend(
            [
                f"Dialogue: 1,{start},{end},Counter,,0,0,0,,CUE {cue.index:02d} / {len(cues):02d}",
                f"Dialogue: 1,{start},{end},Category,,0,0,0,,{{\\c{color}}}{ass_escape(category)}  //  {ass_escape(status)}  //  NOW AUDIBLE",
                f"Dialogue: 1,{start},{end},Cue,,0,0,0,,{ass_escape(cue_display_name(cue.name))}",
                f"Dialogue: 1,{start},{end},Meta,,0,0,0,,{ass_escape(meta)}",
                f"Dialogue: 1,{start},{end},NextHead,,0,0,0,,NEXT ON THE CUE SHEET",
                f"Dialogue: 1,{start},{end},Next,,0,0,0,,{ass_escape(chr(10).join(next_lines))}",
            ]
        )
    path.write_text(header + "\n".join(events) + "\n", encoding="utf-8")


def ffmpeg_escape(path: Path) -> str:
    return str(path.resolve()).replace("\\", r"\\").replace(":", r"\:").replace("'", r"\'")


def probe_duration(ffprobe: str, path: Path) -> float:
    result = subprocess.run(
        [
            ffprobe,
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            str(path),
        ],
        check=True,
        capture_output=True,
        text=True,
    )
    return float(result.stdout.strip())


def probe_media(ffprobe: str, path: Path) -> dict:
    result = subprocess.run(
        [ffprobe, "-v", "error", "-show_streams", "-show_format", "-of", "json", str(path)],
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(result.stdout)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("audition_dir", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--ffmpeg", default=shutil.which("ffmpeg"))
    parser.add_argument("--ffprobe", default=shutil.which("ffprobe"))
    parser.add_argument("--fonts-dir", type=Path, default=Path("assets/fonts"))
    args = parser.parse_args()
    if not args.ffmpeg or not args.ffprobe:
        parser.error("ffmpeg and ffprobe are required")

    root = args.audition_dir.resolve()
    cues_path = root / "AUDITION_CUES.csv"
    audio_path = root / "Kessel_full_sound_audition.wav"
    output = (args.output or root / "Kessel_labeled_sound_audition.mp4").resolve()
    ass_path = root / "Kessel_labeled_sound_audition.ass"
    cues = parse_cues(cues_path)
    duration = probe_duration(args.ffprobe, audio_path)
    if cues[-1].end > duration + 0.02:
        raise ValueError("last cue extends beyond master WAV")
    write_ass(ass_path, cues, duration)

    fonts_dir = args.fonts_dir.resolve()
    filter_graph = (
        "[1:a]showwaves=s=1760x225:mode=cline:colors=0xF28C46@0.95:r=30,"
        "format=rgba[wave];"
        "[0:v][wave]overlay=80:735[waveform];"
        f"[waveform]subtitles=filename='{ffmpeg_escape(ass_path)}':"
        f"fontsdir='{ffmpeg_escape(fonts_dir)}'[video]"
    )
    command = [
        args.ffmpeg,
        "-hide_banner",
        "-loglevel",
        "warning",
        "-y",
        "-f",
        "lavfi",
        "-i",
        f"color=c=0x080A0D:s=1920x1080:r=30:d={duration:.6f}",
        "-i",
        str(audio_path),
        "-filter_complex",
        filter_graph,
        "-map",
        "[video]",
        "-map",
        "1:a:0",
        "-c:v",
        "libx264",
        "-preset",
        "medium",
        "-crf",
        "18",
        "-pix_fmt",
        "yuv420p",
        "-c:a",
        "aac",
        "-b:a",
        "320k",
        "-movflags",
        "+faststart",
        "-shortest",
        str(output),
    ]
    subprocess.run(command, check=True)

    video_duration = probe_duration(args.ffprobe, output)
    if abs(video_duration - duration) > 0.08:
        raise RuntimeError(
            f"video duration drifted from source: {video_duration:.3f}s vs {duration:.3f}s"
        )
    probe = probe_media(args.ffprobe, output)
    streams = probe["streams"]
    video_stream = next(stream for stream in streams if stream["codec_type"] == "video")
    audio_stream = next(stream for stream in streams if stream["codec_type"] == "audio")
    status_counts: dict[str, int] = {}
    for cue in cues:
        status_counts[cue_status(cue)] = status_counts.get(cue_status(cue), 0) + 1
    manifest = {
        "schema": "kessel.labeled-sound-audition",
        "version": 1,
        "cue_count": len(cues),
        "duration_seconds": round(video_duration, 6),
        "resolution": [int(video_stream["width"]), int(video_stream["height"])],
        "frame_rate": video_stream["r_frame_rate"],
        "audio": {
            "codec": audio_stream["codec_name"],
            "sample_rate_hz": int(audio_stream["sample_rate"]),
            "channels": int(audio_stream["channels"]),
            "requested_bit_rate": 320000,
            "probed_average_bit_rate": int(audio_stream.get("bit_rate", 0)),
            "source_policy": "unmodified peak-normalized review master encoded once for video",
        },
        "cue_status_counts": status_counts,
        "source_wav": audio_path.name,
        "source_cues": cues_path.name,
        "video": output.name,
    }
    (root / "Kessel_labeled_sound_audition.json").write_text(
        json.dumps(manifest, indent=2) + "\n", encoding="utf-8"
    )
    print(
        f"[labeled-audition] PASS — {len(cues)} cues, {video_duration:.3f}s, {output}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
