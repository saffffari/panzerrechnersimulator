#!/usr/bin/env python3
"""Measure spatial and temporal firefly artifacts in a capture.

The metric deliberately operates on the renderer-sized image rather than the
delivery encode.  A firefly is a bright pixel whose luminance is substantially
above its 3x3 spatial median and whose 3x3 neighborhood contains only a small
number of other candidates.  Across video frames, exact screen-coordinate
recurrence separates transient sampling spikes from frame-locked artifacts.

Only Python's standard library, Pillow, ffmpeg, and ffprobe are required.
"""

from __future__ import annotations

import argparse
import json
import math
import subprocess
import sys
from array import array
from pathlib import Path
from typing import BinaryIO, Iterator

from PIL import Image, ImageChops, ImageFilter


VIDEO_EXTENSIONS = {".mp4", ".mov", ".mkv", ".webm", ".avi", ".m4v"}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path, help="Video or still image to audit")
    parser.add_argument(
        "--start",
        type=float,
        default=0.0,
        help="Video start time in seconds (default: 0)",
    )
    parser.add_argument(
        "--duration",
        type=float,
        default=4.0,
        help="Video duration to sample in seconds (default: 4)",
    )
    parser.add_argument(
        "--fps",
        type=float,
        default=24.0,
        help="Temporal sampling rate for video (default: 24)",
    )
    parser.add_argument(
        "--width",
        type=int,
        default=640,
        help="Analysis width; height preserves source aspect (default: 640)",
    )
    parser.add_argument(
        "--min-luma",
        type=int,
        default=180,
        help="Minimum 8-bit luminance for a candidate (default: 180)",
    )
    parser.add_argument(
        "--min-delta",
        type=int,
        default=60,
        help="Minimum luminance above the local 3x3 median (default: 60)",
    )
    parser.add_argument(
        "--max-neighbors",
        type=int,
        default=3,
        help="Maximum candidate pixels in the candidate's 3x3 area (default: 3)",
    )
    parser.add_argument(
        "--lock-fraction",
        type=float,
        default=0.5,
        help="Fraction of frames required to call a coordinate locked (default: .5)",
    )
    parser.add_argument(
        "--heatmap-output",
        type=Path,
        help="Optional PNG showing outlier recurrence over the first analysis frame",
    )
    parser.add_argument(
        "--json-output",
        type=Path,
        help="Optional path to save the same JSON emitted on stdout",
    )
    parser.add_argument(
        "--pretty",
        action="store_true",
        help="Pretty-print JSON output",
    )
    args = parser.parse_args()

    if not args.input.is_file():
        parser.error(f"input does not exist or is not a file: {args.input}")
    if args.start < 0.0 or args.duration <= 0.0 or args.fps <= 0.0:
        parser.error("--start must be non-negative; --duration and --fps must be positive")
    if args.width <= 0:
        parser.error("--width must be positive")
    if not 0 <= args.min_luma <= 255 or not 0 <= args.min_delta <= 255:
        parser.error("--min-luma and --min-delta must be in [0, 255]")
    if not 1 <= args.max_neighbors <= 9:
        parser.error("--max-neighbors must be in [1, 9]")
    if not 0.0 < args.lock_fraction <= 1.0:
        parser.error("--lock-fraction must be in (0, 1]")
    return args


def probe_dimensions(path: Path) -> tuple[int, int]:
    command = [
        "ffprobe",
        "-v",
        "error",
        "-select_streams",
        "v:0",
        "-show_entries",
        "stream=width,height",
        "-of",
        "csv=p=0:s=x",
        str(path),
    ]
    result = subprocess.run(command, check=True, capture_output=True, text=True)
    width_text, height_text = result.stdout.strip().split("x", maxsplit=1)
    return int(width_text), int(height_text)


def read_exact(stream: BinaryIO, byte_count: int) -> bytes:
    chunks: list[bytes] = []
    remaining = byte_count
    while remaining:
        chunk = stream.read(remaining)
        if not chunk:
            break
        chunks.append(chunk)
        remaining -= len(chunk)
    return b"".join(chunks)


def video_frames(
    path: Path,
    start: float,
    duration: float,
    fps: float,
    width: int,
    height: int,
) -> Iterator[Image.Image]:
    frame_bytes = width * height * 3
    command = [
        "ffmpeg",
        "-hide_banner",
        "-loglevel",
        "error",
        "-i",
        str(path),
        "-ss",
        f"{start:.6f}",
        "-t",
        f"{duration:.6f}",
        "-an",
        "-vf",
        f"fps={fps:.8g},scale={width}:{height}:flags=area",
        "-pix_fmt",
        "rgb24",
        "-f",
        "rawvideo",
        "pipe:1",
    ]
    process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    assert process.stdout is not None
    assert process.stderr is not None
    try:
        while True:
            payload = read_exact(process.stdout, frame_bytes)
            if not payload:
                break
            if len(payload) != frame_bytes:
                raise RuntimeError(
                    f"ffmpeg returned a partial RGB frame ({len(payload)}/{frame_bytes} bytes)"
                )
            yield Image.frombytes("RGB", (width, height), payload)
    finally:
        process.stdout.close()
        stderr = process.stderr.read().decode("utf-8", errors="replace")
        return_code = process.wait()
        process.stderr.close()
        if return_code != 0:
            raise RuntimeError(f"ffmpeg failed with exit code {return_code}: {stderr.strip()}")


def still_frame(path: Path, width: int, height: int) -> Image.Image:
    with Image.open(path) as source:
        return source.convert("RGB").resize((width, height), Image.Resampling.BOX)


def candidate_mask(
    frame: Image.Image,
    min_luma: int,
    min_delta: int,
    max_neighbors: int,
) -> Image.Image:
    luma = frame.convert("L")
    median = luma.filter(ImageFilter.MedianFilter(size=3))
    positive_delta = ImageChops.subtract(luma, median)

    bright = luma.point([0] * min_luma + [255] * (256 - min_luma))
    contrast = positive_delta.point([0] * min_delta + [255] * (256 - min_delta))
    candidates = ImageChops.multiply(bright, contrast)

    unit_candidates = candidates.point([0] + [1] * 255)
    neighbor_count = unit_candidates.filter(
        ImageFilter.Kernel((3, 3), [1] * 9, scale=1, offset=0)
    )
    sparse = neighbor_count.point(
        [255 if 1 <= count <= max_neighbors else 0 for count in range(256)]
    )
    return ImageChops.multiply(candidates, sparse)


def mask_count(mask: Image.Image) -> int:
    return mask.histogram()[255]


def percentile(values: list[int], quantile: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    index = (len(ordered) - 1) * quantile
    lower = math.floor(index)
    upper = math.ceil(index)
    if lower == upper:
        return float(ordered[lower])
    fraction = index - lower
    return ordered[lower] * (1.0 - fraction) + ordered[upper] * fraction


def make_heatmap(
    background: Image.Image,
    occupancy: array,
    frame_count: int,
    output: Path,
) -> None:
    alpha = bytearray(len(occupancy))
    for index, count in enumerate(occupancy):
        if count:
            alpha[index] = min(230, 48 + round(182 * count / max(frame_count, 1)))
    overlay = Image.new("RGBA", background.size, (255, 20, 0, 255))
    overlay.putalpha(Image.frombytes("L", background.size, bytes(alpha)))
    result = Image.alpha_composite(background.convert("RGBA"), overlay)
    output.parent.mkdir(parents=True, exist_ok=True)
    result.save(output)


def audit(args: argparse.Namespace) -> dict[str, object]:
    source_width, source_height = probe_dimensions(args.input)
    analysis_width = args.width
    analysis_height = max(1, round(source_height * analysis_width / source_width))
    is_video = args.input.suffix.lower() in VIDEO_EXTENSIONS

    if is_video:
        frames = video_frames(
            args.input,
            args.start,
            args.duration,
            args.fps,
            analysis_width,
            analysis_height,
        )
    else:
        frames = iter([still_frame(args.input, analysis_width, analysis_height)])

    pixel_count = analysis_width * analysis_height
    occupancy = array("H", [0]) * pixel_count
    frame_counts: list[int] = []
    adjacent_recurrence_events = 0
    adjacent_denominator_events = 0
    previous_mask: Image.Image | None = None
    first_frame: Image.Image | None = None

    for frame in frames:
        if first_frame is None:
            first_frame = frame.copy()
        mask = candidate_mask(
            frame,
            min_luma=args.min_luma,
            min_delta=args.min_delta,
            max_neighbors=args.max_neighbors,
        )
        count = mask_count(mask)
        frame_counts.append(count)
        mask_bytes = mask.tobytes()
        for index, value in enumerate(mask_bytes):
            if value:
                occupancy[index] += 1

        if previous_mask is not None:
            adjacent_recurrence_events += mask_count(
                ImageChops.multiply(mask, previous_mask)
            )
            adjacent_denominator_events += count
        previous_mask = mask

    if not frame_counts or first_frame is None:
        raise RuntimeError("no video frames were decoded for the requested interval")

    frame_count = len(frame_counts)
    total_events = sum(frame_counts)
    unique_pixels = sum(1 for count in occupancy if count)
    one_frame_pixels = sum(1 for count in occupancy if count == 1)
    recurring_pixels = sum(1 for count in occupancy if count >= 2)

    temporal: dict[str, object] | None
    if frame_count > 1:
        lock_frames = max(2, math.ceil(args.lock_fraction * frame_count))
        locked_pixels = sum(1 for count in occupancy if count >= lock_frames)
        locked_events = sum(count for count in occupancy if count >= lock_frames)
        temporal = {
            "lock_fraction": args.lock_fraction,
            "lock_frame_threshold": lock_frames,
            "frame_locked_pixels": locked_pixels,
            "frame_locked_pixels_ppm": locked_pixels * 1_000_000.0 / pixel_count,
            "frame_locked_event_fraction": (
                locked_events / total_events if total_events else 0.0
            ),
            "frame_locked_events": locked_events,
            "non_locked_events": total_events - locked_events,
            "mean_non_locked_events_per_frame": (total_events - locked_events)
            / frame_count,
            "adjacent_exact_recurrence_fraction": (
                adjacent_recurrence_events / adjacent_denominator_events
                if adjacent_denominator_events
                else 0.0
            ),
            "unique_outlier_coordinates": unique_pixels,
            "one_frame_only_coordinates": one_frame_pixels,
            "recurring_coordinates": recurring_pixels,
            "one_frame_coordinate_fraction": (
                one_frame_pixels / unique_pixels if unique_pixels else 0.0
            ),
        }
    else:
        temporal = None

    if args.heatmap_output:
        make_heatmap(first_frame, occupancy, frame_count, args.heatmap_output)

    return {
        "source": str(args.input.resolve()),
        "source_kind": "video" if is_video else "still",
        "source_dimensions": [source_width, source_height],
        "analysis_dimensions": [analysis_width, analysis_height],
        "video_window": (
            {"start_seconds": args.start, "duration_seconds": args.duration, "fps": args.fps}
            if is_video
            else None
        ),
        "frames_analyzed": frame_count,
        "detector": {
            "minimum_luma_8bit": args.min_luma,
            "minimum_delta_above_3x3_median_8bit": args.min_delta,
            "maximum_candidates_in_3x3": args.max_neighbors,
        },
        "spatial": {
            "total_isolated_outlier_events": total_events,
            "mean_isolated_outliers_per_frame": total_events / frame_count,
            "median_isolated_outliers_per_frame": percentile(frame_counts, 0.5),
            "p95_isolated_outliers_per_frame": percentile(frame_counts, 0.95),
            "maximum_isolated_outliers_in_frame": max(frame_counts),
            "mean_outlier_density_ppm": total_events * 1_000_000.0 / (frame_count * pixel_count),
            "per_frame_counts": frame_counts,
        },
        "temporal": temporal,
        "heatmap_output": str(args.heatmap_output.resolve()) if args.heatmap_output else None,
        "json_output": str(args.json_output.resolve()) if args.json_output else None,
    }


def main() -> int:
    args = parse_args()
    try:
        result = audit(args)
    except (OSError, RuntimeError, subprocess.CalledProcessError, ValueError) as error:
        print(f"firefly_audit: {error}", file=sys.stderr)
        return 1
    indent = 2 if args.pretty else None
    payload = json.dumps(result, indent=indent, sort_keys=True)
    if args.json_output:
        args.json_output.parent.mkdir(parents=True, exist_ok=True)
        args.json_output.write_text(payload + "\n", encoding="utf-8")
    print(payload)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
