#!/usr/bin/env python3
import argparse
import hashlib
import json
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

from compare_framemd5 import compare_framemd5, parse_framemd5_text
from compare_probe_json import compare_probe, normalize_ffprobe_document

ROOT = Path(__file__).resolve().parents[2]
SAMPLES = ROOT / "harness" / "samples"
REFERENCE = ROOT / "harness" / "reference"
CORRECTNESS = ROOT / "site" / "data" / "correctness.json"


def main():
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("reference")
    run_parser = subparsers.add_parser("run")
    run_parser.add_argument("--rmpeg-probe", default=str(ROOT / "target" / "release" / "rmpeg-probe"))
    run_parser.add_argument("--rmpeg", default=str(ROOT / "target" / "release" / "rmpeg"))
    run_parser.add_argument("--output", default=str(CORRECTNESS))
    args = parser.parse_args()

    if args.command == "reference":
        return generate_reference()
    return run_tests(Path(args.rmpeg_probe), Path(args.rmpeg), Path(args.output))


def generate_reference():
    require_tool("ffprobe")
    require_tool("ffmpeg")
    samples = sample_paths()
    if not samples:
        raise SystemExit(f"no samples found in {SAMPLES}; run cargo xtask samples first")
    REFERENCE.mkdir(parents=True, exist_ok=True)
    for path in REFERENCE.glob("*.json"):
        path.unlink()
    for sample in samples:
        write_probe_reference(sample)
        if audio_decode_supported(sample):
            write_audio_framemd5_reference(sample)
        if image_decode_supported(sample):
            write_native_framemd5_reference(
                f"{stem(sample)}.image", sample, ["-i", str(sample), "-map", "0:v:0"]
            )
        if video_decode_supported(sample):
            write_native_framemd5_reference(
                f"{stem(sample)}.video", sample, ["-i", str(sample), "-map", "0:v:0"]
            )
    write_capability_references()
    print(f"generated references in {REFERENCE}")
    return 0


def require_tool(name):
    if shutil.which(name) is None:
        raise SystemExit(f"required tool missing: {name}")


def sample_paths():
    paths = []
    for pattern in ("*.wav", "*.mp3", "*.mp4", "*.flac", "*.ogg", "*.png"):
        paths.extend(path for path in SAMPLES.glob(pattern) if path.is_file())
    return sorted(paths)


def stem(path):
    return path.stem


def sample_format(path):
    if path.suffix == ".wav":
        return "wav"
    if path.suffix == ".mp3":
        return "mp3"
    if path.suffix == ".mp4":
        return "mp4"
    if path.suffix == ".flac":
        return "flac"
    if path.suffix == ".ogg":
        return "ogg"
    if path.suffix == ".png":
        return "png"
    return path.suffix.lstrip(".") or "unknown"


def audio_decode_supported(path):
    return path.suffix in {".wav", ".mp3", ".mp4", ".flac", ".ogg"}


def image_decode_supported(path):
    return path.suffix == ".png"


def video_decode_supported(path):
    return path.suffix == ".mp4"


def write_json(path, document):
    path.write_text(json.dumps(document, indent=2, sort_keys=True) + "\n")


def write_probe_reference(sample):
    result = subprocess.run(
        [
            "ffprobe",
            "-v",
            "error",
            "-show_format",
            "-show_streams",
            "-of",
            "json",
            str(sample),
        ],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    output = {
        "sample": sample.name,
        "kind": "probe-json",
        "command": result.args,
        "expected_error": result.returncode != 0,
    }
    if result.returncode == 0:
        output["probe"] = normalize_ffprobe_document(json.loads(result.stdout))
    else:
        output["returncode"] = result.returncode
        output["stderr"] = trim(result.stderr)
    write_json(REFERENCE / f"{stem(sample)}.probe.json", output)


def write_audio_framemd5_reference(sample):
    probe_result = subprocess.run(
        [
            "ffprobe",
            "-v",
            "error",
            "-show_format",
            "-show_streams",
            "-of",
            "json",
            str(sample),
        ],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    result = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", str(sample), "-f", "s16le", "-"],
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    output = {
        "sample": sample.name,
        "kind": "framemd5",
        "command": result.args,
        "expected_error": result.returncode != 0,
    }
    if result.returncode == 0:
        if probe_result.returncode != 0:
            output["expected_error"] = True
            output["returncode"] = probe_result.returncode
            output["stderr"] = trim(probe_result.stderr)
        else:
            probe = normalize_ffprobe_document(json.loads(probe_result.stdout))
            stream = first_audio_stream(probe)
            output["frames"] = normalized_framemd5_frames(
                result.stdout,
                int(stream["sample_rate"]),
                int(stream["channels"]),
            )
    else:
        output["returncode"] = result.returncode
        output["stderr"] = trim(result.stderr.decode("utf-8", errors="replace"))
    write_json(REFERENCE / f"{stem(sample)}.framemd5.json", output)


def write_native_framemd5_reference(key, sample, ffmpeg_args):
    result = subprocess.run(
        ["ffmpeg", "-v", "error", *ffmpeg_args, "-f", "framemd5", "-"],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    output = {
        "sample": sample.name,
        "kind": "framemd5",
        "command": result.args,
        "expected_error": result.returncode != 0,
    }
    if result.returncode == 0:
        output["frames"] = parse_framemd5_text(result.stdout)
    else:
        output["returncode"] = result.returncode
        output["stderr"] = trim(result.stderr)
    write_json(REFERENCE / f"{key}.framemd5.json", output)


def write_capability_references():
    flac = SAMPLES / "tone_flac.flac"
    if not flac.exists():
        return
    audio = audio_stream_metadata(flac)
    write_audio_transform_reference(
        "filter_audio_volume_flac",
        flac,
        ["-i", str(flac), "-filter:a", "volume=0.5"],
        int(audio["sample_rate"]),
        int(audio["channels"]),
    )
    write_audio_transform_reference(
        "seek_audio_flac_100ms",
        flac,
        ["-ss", "0.1", "-i", str(flac)],
        int(audio["sample_rate"]),
        int(audio["channels"]),
    )
    write_audio_transform_reference(
        "resample_audio_flac_16000",
        flac,
        ["-i", str(flac), "-ar", "16000"],
        16000,
        int(audio["channels"]),
    )
    write_binary_reference(
        "remux_flac_wav",
        flac,
        ["-i", str(flac), "-f", "wav"],
    )


def audio_stream_metadata(sample):
    result = subprocess.run(
        [
            "ffprobe",
            "-v",
            "error",
            "-show_format",
            "-show_streams",
            "-of",
            "json",
            str(sample),
        ],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if result.returncode != 0:
        raise RuntimeError(trim(result.stderr))
    probe = normalize_ffprobe_document(json.loads(result.stdout))
    return first_audio_stream(probe)


def write_audio_transform_reference(key, sample, ffmpeg_args, sample_rate, channels):
    result = subprocess.run(
        ["ffmpeg", "-v", "error", *ffmpeg_args, "-f", "s16le", "-"],
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    output = {
        "sample": sample.name,
        "kind": "framemd5",
        "command": result.args,
        "expected_error": result.returncode != 0,
    }
    if result.returncode == 0:
        output["frames"] = normalized_framemd5_frames(result.stdout, sample_rate, channels)
    else:
        output["returncode"] = result.returncode
        output["stderr"] = trim(result.stderr.decode("utf-8", errors="replace"))
    write_json(REFERENCE / f"{key}.framemd5.json", output)


def write_binary_reference(key, sample, ffmpeg_args):
    result = subprocess.run(
        ["ffmpeg", "-v", "error", *ffmpeg_args, "-"],
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    output = {
        "sample": sample.name,
        "kind": "binary",
        "command": result.args,
        "expected_error": result.returncode != 0,
    }
    if result.returncode == 0:
        output["size"] = len(result.stdout)
        output["hash"] = hashlib.md5(result.stdout).hexdigest()
    else:
        output["returncode"] = result.returncode
        output["stderr"] = trim(result.stderr.decode("utf-8", errors="replace"))
    write_json(REFERENCE / f"{key}.binary.json", output)


def first_audio_stream(probe):
    for stream in probe["streams"]:
        if stream.get("codec_type") == "audio":
            return stream
    raise ValueError("ffprobe output did not include an audio stream")


def normalized_framemd5_frames(decoded_pcm, sample_rate, channels):
    output_block_align = channels * 2
    samples_per_frame = wav_framemd5_samples_per_frame(sample_rate)
    total_samples = len(decoded_pcm) // output_block_align
    frames = []
    pts = 0
    offset = 0
    while pts < total_samples:
        duration = min(samples_per_frame, total_samples - pts)
        size = duration * output_block_align
        payload = decoded_pcm[offset : offset + size]
        frames.append(
            {
                "stream_index": 0,
                "dts": pts,
                "pts": pts,
                "duration": duration,
                "size": size,
                "hash": hashlib.md5(payload).hexdigest(),
            }
        )
        pts += duration
        offset += size
    return frames


def wav_framemd5_samples_per_frame(sample_rate):
    target = sample_rate // 10
    if target <= 0:
        return 1
    samples = 1
    while samples <= target // 2:
        samples *= 2
    return samples


def run_tests(rmpeg_probe, rmpeg, output_path):
    output_path.parent.mkdir(parents=True, exist_ok=True)
    tests = []
    samples = sample_paths()
    if not samples:
        tests.append(
            {
                "name": "sample generation",
                "kind": "setup",
                "status": "error",
                "details": f"no samples found in {SAMPLES}; run cargo xtask samples first",
            }
        )
    for sample in samples:
        tests.append(run_probe_test(rmpeg_probe, sample))
        if audio_decode_supported(sample):
            tests.append(run_audio_framemd5_test(rmpeg, sample))
        if image_decode_supported(sample):
            tests.append(run_image_framemd5_test(rmpeg, sample))
        if video_decode_supported(sample):
            tests.append(run_video_framemd5_test(rmpeg, sample))
    tests.extend(run_capability_tests(rmpeg))

    document = {
        "generated_at": now(),
        "rmpeg_commit": git_commit(),
        "ffmpeg_version": ffmpeg_version(),
        "summary": summarize(tests),
        "tests": tests,
    }
    write_json(output_path, document)
    print(f"wrote {output_path}")
    print(json.dumps(document["summary"], sort_keys=True))
    return 0


def run_probe_test(rmpeg_probe, sample):
    name = f"probe {sample_format(sample)} {stem(sample)}"
    ref_path = REFERENCE / f"{stem(sample)}.probe.json"
    if not ref_path.exists():
        return errored(name, "probe-json", f"missing reference {ref_path}")
    ref = json.loads(ref_path.read_text())
    result = subprocess.run(
        [str(rmpeg_probe), str(sample)],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if ref.get("expected_error"):
        if result.returncode != 0:
            return passed(name, "probe-json", "both ffprobe and rmpeg-probe rejected input")
        return failed(name, "probe-json", "ffprobe rejected input but rmpeg-probe accepted it")
    if result.returncode != 0:
        return failed(name, "probe-json", trim(result.stderr))
    try:
        actual = json.loads(result.stdout)
        ok, details = compare_probe(ref["probe"], actual)
    except Exception as error:
        return errored(name, "probe-json", str(error))
    return passed(name, "probe-json") if ok else failed(name, "probe-json", details)


def run_audio_framemd5_test(rmpeg, sample):
    name = f"decode/hash {sample_format(sample)} {stem(sample)}"
    ref_path = REFERENCE / f"{stem(sample)}.framemd5.json"
    return run_framemd5_command(
        name,
        "framemd5",
        ref_path,
        [str(rmpeg), "decode", str(sample), "--framemd5"],
    )


def run_image_framemd5_test(rmpeg, sample):
    name = f"decode/image {sample_format(sample)} {stem(sample)}"
    ref_path = REFERENCE / f"{stem(sample)}.image.framemd5.json"
    return run_framemd5_command(
        name,
        "framemd5",
        ref_path,
        [str(rmpeg), "decode-image", str(sample), "--framemd5"],
    )


def run_video_framemd5_test(rmpeg, sample):
    name = f"decode/video {sample_format(sample)} {stem(sample)}"
    ref_path = REFERENCE / f"{stem(sample)}.video.framemd5.json"
    return run_framemd5_command(
        name,
        "framemd5",
        ref_path,
        [str(rmpeg), "decode-video", str(sample), "--framemd5"],
    )


def run_capability_tests(rmpeg):
    flac = SAMPLES / "tone_flac.flac"
    return [
        run_framemd5_command(
            "filter audio volume tone_flac",
            "filter",
            REFERENCE / "filter_audio_volume_flac.framemd5.json",
            [str(rmpeg), "filter", str(flac), "--volume", "0.5", "--framemd5"],
        ),
        run_framemd5_command(
            "seek audio tone_flac 100ms",
            "seek",
            REFERENCE / "seek_audio_flac_100ms.framemd5.json",
            [str(rmpeg), "seek", str(flac), "--start", "0.1", "--framemd5"],
        ),
        run_framemd5_command(
            "resample audio tone_flac 16000",
            "resample",
            REFERENCE / "resample_audio_flac_16000.framemd5.json",
            [str(rmpeg), "resample", str(flac), "--sample-rate", "16000", "--framemd5"],
        ),
        run_binary_command(
            "remux flac tone_flac wav",
            "remux",
            REFERENCE / "remux_flac_wav.binary.json",
            [str(rmpeg), "remux", str(flac), "--format", "wav", "--output", "-"],
        ),
    ]


def run_framemd5_command(name, kind, ref_path, command):
    if not ref_path.exists():
        return errored(name, kind, f"missing reference {ref_path}")
    ref = json.loads(ref_path.read_text())
    result = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if ref.get("expected_error"):
        if result.returncode != 0:
            return passed(name, kind, "both ffmpeg and rmpeg rejected input")
        return failed(name, kind, "ffmpeg rejected input but rmpeg accepted it")
    if result.returncode != 0:
        return failed(name, kind, trim(result.stderr))
    try:
        ok, details = compare_framemd5(ref["frames"], result.stdout)
    except Exception as error:
        return errored(name, kind, str(error))
    return passed(name, kind) if ok else failed(name, kind, details)


def run_binary_command(name, kind, ref_path, command):
    if not ref_path.exists():
        return errored(name, kind, f"missing reference {ref_path}")
    ref = json.loads(ref_path.read_text())
    result = subprocess.run(
        command,
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if ref.get("expected_error"):
        if result.returncode != 0:
            return passed(name, kind, "both ffmpeg and rmpeg rejected input")
        return failed(name, kind, "ffmpeg rejected input but rmpeg accepted it")
    if result.returncode != 0:
        return failed(name, kind, trim(result.stderr.decode("utf-8", errors="replace")))
    actual_hash = hashlib.md5(result.stdout).hexdigest()
    actual_size = len(result.stdout)
    if ref.get("hash") == actual_hash and ref.get("size") == actual_size:
        return passed(name, kind)
    return failed(
        name,
        kind,
        f"binary output differs: expected {ref.get('size')} bytes/{ref.get('hash')}, got {actual_size} bytes/{actual_hash}",
    )


def passed(name, kind, details=""):
    return {"name": name, "kind": kind, "status": "passed", "details": details}


def failed(name, kind, details):
    return {"name": name, "kind": kind, "status": "failed", "details": details}


def errored(name, kind, details):
    return {"name": name, "kind": kind, "status": "error", "details": details}


def summarize(tests):
    return {
        "total": len(tests),
        "passed": sum(1 for test in tests if test["status"] == "passed"),
        "failed": sum(1 for test in tests if test["status"] == "failed"),
        "skipped": sum(1 for test in tests if test["status"] == "skipped"),
        "errors": sum(1 for test in tests if test["status"] == "error"),
    }


def now():
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def git_commit():
    result = subprocess.run(
        ["git", "rev-parse", "--short", "HEAD"],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
    )
    return result.stdout.strip() if result.returncode == 0 else "unknown"


def ffmpeg_version():
    result = subprocess.run(
        ["ffmpeg", "-version"],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
    )
    if result.returncode != 0:
        return "unknown"
    return result.stdout.splitlines()[0]


def trim(text, limit=500):
    text = " ".join((text or "").split())
    return text[:limit]


if __name__ == "__main__":
    raise SystemExit(main())
