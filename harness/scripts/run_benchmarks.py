#!/usr/bin/env python3
import json
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TINY_WAV = ROOT / "harness" / "samples" / "tiny.wav"
TONE_MP3 = ROOT / "harness" / "samples" / "tone_mp3.mp3"
H264_AAC_MP4 = ROOT / "harness" / "samples" / "h264_aac_mp4.mp4"
TONE_FLAC = ROOT / "harness" / "samples" / "tone_flac.flac"
TONE_OPUS = ROOT / "harness" / "samples" / "tone_opus.ogg"
TONE_VORBIS = ROOT / "harness" / "samples" / "tone_vorbis.ogg"
BENCHMARKS = ROOT / "site" / "data" / "benchmarks.json"
SUMMARY = ROOT / "site" / "data" / "benchmark-summary.json"


def main():
    require_tool("hyperfine")
    require_tool("ffprobe")
    require_tool("ffmpeg")
    for sample in (TINY_WAV, TONE_MP3, H264_AAC_MP4, TONE_FLAC, TONE_OPUS, TONE_VORBIS):
        if not sample.exists():
            raise SystemExit(f"missing {sample}; run cargo xtask samples first")

    BENCHMARKS.parent.mkdir(parents=True, exist_ok=True)
    rmpeg_probe = ROOT / "target" / "release" / "rmpeg-probe"
    rmpeg = ROOT / "target" / "release" / "rmpeg"
    if not rmpeg_probe.exists() or not rmpeg.exists():
        raise SystemExit("missing release binaries; cargo xtask bench should build them first")

    commands = [
        ("ffprobe probe tiny wav", f"ffprobe -v error -show_format -show_streams -of json {TINY_WAV}"),
        ("rmpeg-probe probe tiny wav", f"{rmpeg_probe} {TINY_WAV}"),
        ("ffprobe probe tone mp3", f"ffprobe -v error -show_format -show_streams -of json {TONE_MP3}"),
        ("rmpeg-probe probe tone mp3", f"{rmpeg_probe} {TONE_MP3}"),
        ("ffprobe probe tone flac", f"ffprobe -v error -show_format -show_streams -of json {TONE_FLAC}"),
        ("rmpeg-probe probe tone flac", f"{rmpeg_probe} {TONE_FLAC}"),
        ("ffprobe probe tone opus", f"ffprobe -v error -show_format -show_streams -of json {TONE_OPUS}"),
        ("rmpeg-probe probe tone opus", f"{rmpeg_probe} {TONE_OPUS}"),
        ("ffprobe probe tone vorbis", f"ffprobe -v error -show_format -show_streams -of json {TONE_VORBIS}"),
        ("rmpeg-probe probe tone vorbis", f"{rmpeg_probe} {TONE_VORBIS}"),
        ("ffprobe probe h264 aac mp4", f"ffprobe -v error -show_format -show_streams -of json {H264_AAC_MP4}"),
        ("rmpeg-probe probe h264 aac mp4", f"{rmpeg_probe} {H264_AAC_MP4}"),
        ("ffmpeg framemd5 tiny wav", f"ffmpeg -v error -i {TINY_WAV} -f framemd5 -"),
        ("rmpeg framemd5 tiny wav", f"{rmpeg} decode {TINY_WAV} --framemd5"),
    ]
    args = ["hyperfine", "--warmup", "1", "--min-runs", "3", "--export-json", str(BENCHMARKS)]
    for name, _command in commands:
        args.extend(["--command-name", name])
    args.extend(command for _name, command in commands)

    result = subprocess.run(args, cwd=ROOT, text=True)
    if result.returncode != 0:
        return result.returncode

    write_summary(commands)
    print(f"wrote {BENCHMARKS}")
    print(f"wrote {SUMMARY}")
    return 0


def require_tool(name):
    if shutil.which(name) is None:
        raise SystemExit(f"required tool missing: {name}")


def write_summary(commands):
    data = json.loads(BENCHMARKS.read_text())
    by_name = {entry["command"]: entry for entry in data.get("results", [])}
    by_command = {command: by_name.get(name) or by_name.get(command) for name, command in commands}
    benchmarks = []
    benchmarks.append(
        summarize_pair(
            "probe tiny wav",
            by_name.get("ffprobe probe tiny wav") or by_command[commands[0][1]],
            by_name.get("rmpeg-probe probe tiny wav") or by_command[commands[1][1]],
        )
    )
    benchmarks.append(
        summarize_pair(
            "probe tone mp3",
            by_name.get("ffprobe probe tone mp3") or by_command[commands[2][1]],
            by_name.get("rmpeg-probe probe tone mp3") or by_command[commands[3][1]],
        )
    )
    benchmarks.append(
        summarize_pair(
            "probe h264/aac mp4",
            by_name.get("ffprobe probe h264 aac mp4") or by_command[commands[10][1]],
            by_name.get("rmpeg-probe probe h264 aac mp4") or by_command[commands[11][1]],
        )
    )
    benchmarks.append(
        summarize_pair(
            "probe tone flac",
            by_name.get("ffprobe probe tone flac") or by_command[commands[4][1]],
            by_name.get("rmpeg-probe probe tone flac") or by_command[commands[5][1]],
        )
    )
    benchmarks.append(
        summarize_pair(
            "probe tone opus",
            by_name.get("ffprobe probe tone opus") or by_command[commands[6][1]],
            by_name.get("rmpeg-probe probe tone opus") or by_command[commands[7][1]],
        )
    )
    benchmarks.append(
        summarize_pair(
            "probe tone vorbis",
            by_name.get("ffprobe probe tone vorbis") or by_command[commands[8][1]],
            by_name.get("rmpeg-probe probe tone vorbis") or by_command[commands[9][1]],
        )
    )
    benchmarks.append(
        summarize_pair(
            "framemd5 tiny wav",
            by_name.get("ffmpeg framemd5 tiny wav") or by_command[commands[12][1]],
            by_name.get("rmpeg framemd5 tiny wav") or by_command[commands[13][1]],
        )
    )
    SUMMARY.write_text(
        json.dumps(
            {"generated_at": now(), "benchmarks": benchmarks},
            indent=2,
            sort_keys=True,
        )
        + "\n"
    )


def summarize_pair(name, ffmpeg_result, rmpeg_result):
    if not ffmpeg_result or not rmpeg_result:
        return {
            "name": name,
            "ffmpeg_seconds_mean": 0.0,
            "rmpeg_seconds_mean": 0.0,
            "relative": "not measured",
            "status": "skipped",
        }
    ffmpeg_mean = float(ffmpeg_result["mean"])
    rmpeg_mean = float(rmpeg_result["mean"])
    return {
        "name": name,
        "ffmpeg_seconds_mean": ffmpeg_mean,
        "rmpeg_seconds_mean": rmpeg_mean,
        "relative": relative(ffmpeg_mean, rmpeg_mean),
        "status": "measured",
    }


def relative(ffmpeg_mean, rmpeg_mean):
    if ffmpeg_mean <= 0 or rmpeg_mean <= 0:
        return "not comparable"
    ratio = rmpeg_mean / ffmpeg_mean
    if ratio >= 1:
        return f"rmpeg is {ratio:.2f}x slower"
    return f"rmpeg is {1 / ratio:.2f}x faster"


def now():
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


if __name__ == "__main__":
    raise SystemExit(main())
