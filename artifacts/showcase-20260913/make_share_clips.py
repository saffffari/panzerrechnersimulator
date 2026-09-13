#!/usr/bin/env python3
"""Reproduce local share cuts and technical checks. Never overwrite source or output media.

Run from any directory: python artifacts/showcase-20260913/make_share_clips.py
Use --verify-only to inspect existing outputs. This does not perform perceptual audio QA.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
from decimal import Decimal, ROUND_CEILING
import hashlib
import json
import math
from pathlib import Path
import struct
import subprocess

import numpy as np

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
EDIT_LIST = HERE / "share-edit-list.v1.json"


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as source:
        for data in iter(lambda: source.read(1024 * 1024), b""):
            h.update(data)
    return h.hexdigest()


def probe(path):
    return json.loads(subprocess.check_output([
        "ffprobe", "-v", "error", "-show_streams", "-show_format", "-of", "json", str(path)
    ], text=True))


def frame_ceiling(seconds, fps):
    return int((Decimal(str(seconds)) * fps).to_integral_value(rounding=ROUND_CEILING))


def actual_segments(clip, fps):
    segments = []
    for start, end in clip["segments"]:
        a, b = frame_ceiling(start, fps), frame_ceiling(end, fps)
        assert b > a >= 0
        segments.append({"requested_seconds": [start, end], "source_frames": [a, b],
                         "actual_seconds": [a / fps, b / fps], "frames": b - a,
                         "duration_seconds": (b - a) / fps})
    return segments


def command_for(source, output, segments, fps, fade):
    filters = []
    for index, segment in enumerate(segments):
        first, end = segment["source_frames"]
        start_s, end_s = segment["actual_seconds"]
        duration = segment["duration_seconds"]
        filters.append(f"[0:v:0]trim=start_frame={first}:end_frame={end},"
                       f"setpts=PTS-STARTPTS[v{index}]")
        filters.append(f"[0:a:0]atrim=start={start_s:.12f}:end={end_s:.12f},"
                       f"asetpts=PTS-STARTPTS,apad=whole_dur={duration:.12f},"
                       f"atrim=duration={duration:.12f},afade=t=in:st=0:d={fade:.6f},"
                       f"afade=t=out:st={duration-fade:.12f}:d={fade:.6f}[a{index}]")
    if len(segments) > 1:
        labels = "".join(f"[v{i}][a{i}]" for i in range(len(segments)))
        filters.append(f"{labels}concat=n={len(segments)}:v=1:a=1[vout][aout]")
        video, audio = "[vout]", "[aout]"
    else:
        video, audio = "[v0]", "[a0]"
    return ["ffmpeg", "-hide_banner", "-loglevel", "warning", "-n", "-threads", "2",
            "-i", str(source), "-filter_complex_threads", "1", "-filter_complex", ";".join(filters),
            "-map", video, "-map", audio, "-c:v", "libx264", "-preset", "medium", "-crf", "14",
            "-threads", "2", "-pix_fmt", "yuv420p", "-r", str(fps), "-fps_mode", "cfr",
            "-c:a", "aac", "-b:a", "192k", "-ar", "44100", "-ac", "2",
            "-movflags", "+faststart", "-map_metadata", "0", str(output)]


def atoms(path):
    result = []
    with path.open("rb") as stream:
        while True:
            position = stream.tell()
            header = stream.read(8)
            if len(header) != 8:
                break
            size, kind = struct.unpack(">I4s", header)
            header_size = 8
            if size == 1:
                size = struct.unpack(">Q", stream.read(8))[0]
                header_size = 16
            result.append({"type": kind.decode("ascii"), "offset": position, "size": size})
            if not size:
                break
            assert size >= header_size
            stream.seek(position + size)
    return result


def verify(path, expected_frames, fps):
    metadata = probe(path)
    video = next(s for s in metadata["streams"] if s["codec_type"] == "video")
    audio = next(s for s in metadata["streams"] if s["codec_type"] == "audio")
    assert video["codec_name"] == "h264" and video["pix_fmt"] == "yuv420p"
    assert (video["width"], video["height"]) == (1920, 1080)
    assert video["r_frame_rate"] == f"{fps}/1" and int(video["nb_frames"]) == expected_frames
    assert audio["codec_name"] == "aac" and int(audio["channels"]) == 2
    assert int(audio["sample_rate"]) == 44100
    assert abs(float(metadata["format"]["duration"]) - expected_frames / fps) < .1
    boxes = atoms(path)
    positions = {box["type"]: box["offset"] for box in boxes}
    assert positions["moov"] < positions["mdat"], "MP4 is not faststart"
    # Full decode catches corrupt packet/stream endings. No realtime playback or game process.
    decoded = subprocess.run(["ffmpeg", "-v", "error", "-xerror", "-threads", "2", "-i", str(path),
                              "-map", "0:v:0", "-map", "0:a:0", "-f", "null", "-"],
                             capture_output=True, text=True)
    assert decoded.returncode == 0, decoded.stderr
    raw = subprocess.check_output(["ffmpeg", "-v", "error", "-i", str(path), "-map", "0:a:0",
                                   "-c:a", "pcm_f32le", "-f", "f32le", "-"])
    samples = np.frombuffer(raw, dtype="<f4").reshape(-1, 2)
    assert len(samples) > 0 and np.isfinite(samples).all()
    peak = float(np.max(np.abs(samples)))
    rms = float(np.sqrt(np.mean(samples.astype(float) ** 2)))
    assert peak > 1e-5 and rms > 1e-7, "empty/silent audio"
    assert peak < 1.0, f"decoded audio reaches clipping: {peak}"
    thumbnails = []
    for seconds in [0, expected_frames / fps * .5, (expected_frames - 1) / fps]:
        raw_frame = subprocess.check_output([
            "ffmpeg", "-v", "error", "-ss", str(seconds), "-i", str(path), "-frames:v", "1",
            "-vf", "scale=160:90:flags=neighbor", "-pix_fmt", "rgb24", "-f", "rawvideo", "-"
        ])
        pixels = np.frombuffer(raw_frame, dtype=np.uint8)
        assert pixels.size == 160 * 90 * 3 and float(pixels.std()) > 1
        thumbnails.append({"at_s": seconds, "rgb_min": int(pixels.min()), "rgb_max": int(pixels.max()),
                           "rgb_std": float(pixels.std()), "sha256": hashlib.sha256(raw_frame).hexdigest()})
    return {"status": "PASS", "ffprobe": metadata, "mp4_top_level_atoms": boxes,
            "faststart": True, "full_decode": "PASS", "decode_stderr": decoded.stderr,
            "nonempty_video_samples": thumbnails,
            "audio": {"scope": "Decoded numeric checks only; no perceptual listening claim",
                      "sample_frames": len(samples), "finite": True, "peak_linear": peak,
                      "peak_dbfs": 20 * math.log10(peak), "rms_dbfs": 20 * math.log10(rms),
                      "samples_at_or_above_full_scale": int(np.count_nonzero(np.abs(samples) >= 1))}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verify-only", action="store_true")
    parser.add_argument("--jobs", type=int, default=2, choices=[1, 2])
    parser.add_argument("--only", nargs="+", help="Build/check only named clips; preserve existing edits")
    args = parser.parse_args()
    spec = json.loads(EDIT_LIST.read_text())
    if args.only:
        assert set(args.only) <= {clip["name"] for clip in spec["clips"]}, "Unknown clip name"
    selected = [clip for clip in spec["clips"] if not args.only or clip["name"] in args.only]
    for clip in selected:
        source, output = ROOT / clip["source"], ROOT / clip["output"]
        assert source.is_file() and source.resolve() != output.resolve()
        if not args.verify_only:
            assert not output.exists(), f"Refusing to overwrite {output}"

    def make(clip):
        source, output = ROOT / clip["source"], ROOT / clip["output"]
        source_digest = digest(source)
        source_metadata = probe(source)
        segments = actual_segments(clip, spec["fps"])
        assert segments[-1]["actual_seconds"][1] <= float(source_metadata["format"]["duration"]) + .01
        command = command_for(source, output, segments, spec["fps"], spec["audio_fade_seconds"])
        (HERE / f"{clip['name']}-ffmpeg-command.json").write_text(json.dumps(command, indent=2) + "\n")
        if not args.verify_only:
            with (HERE / f"{clip['name']}-encode.log").open("w") as log:
                subprocess.run(command, check=True, stdout=log, stderr=subprocess.STDOUT)
        result = verify(output, sum(s["frames"] for s in segments), spec["fps"])
        assert digest(source) == source_digest, "Source changed during edit"
        receipt = {"name": clip["name"], "source": str(source), "source_sha256": source_digest,
                   "source_unchanged": True, "source_probe": source_metadata, "output": str(output),
                   "output_sha256": digest(output), "output_bytes": output.stat().st_size,
                   "segments": segments, "ffmpeg_command": command, "verification": result}
        (HERE / f"{clip['name']}-share-verification.json").write_text(json.dumps(receipt, indent=2) + "\n")
        print(json.dumps({"clip": clip["name"], "output": str(output), "status": result["status"],
                          "duration_s": float(result["ffprobe"]["format"]["duration"]),
                          "audio_peak_dbfs": result["audio"]["peak_dbfs"]}), flush=True)
        return receipt

    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
        list(pool.map(make, selected))
    receipts = []
    for clip in spec["clips"]:
        receipt_path = HERE / f"{clip['name']}-share-verification.json"
        if receipt_path.exists():
            receipts.append(json.loads(receipt_path.read_text()))
    version = subprocess.check_output(["ffmpeg", "-version"], text=True).splitlines()[0]
    (HERE / "share-edits-verification.json").write_text(json.dumps({
        "status": "PASS" if len(receipts) == len(spec["clips"]) else "PARTIAL",
        "recipe": str(EDIT_LIST), "ffmpeg": version,
        "scope": "Technical decode, file format, sample nonemptiness and numerical audio checks; no perceptual audio audition",
        "clips": receipts}, indent=2) + "\n")


if __name__ == "__main__":
    main()
