#!/usr/bin/env python3
import json
import sys
from pathlib import Path

DEFAULT_DURATION_TOLERANCE_SECONDS = 0.001
COMPRESSED_AUDIO_DURATION_TOLERANCE_SECONDS = 0.05


def normalize_ffprobe_document(document):
    return normalize_probe_document(
        {
            "format": normalize_format_name(document.get("format", {}).get("format_name", "")),
            "streams": [
                normalize_ffprobe_stream(stream, output_index)
                for output_index, stream in enumerate(document.get("streams", []))
                if stream.get("codec_type") in {"audio", "video"}
            ],
        }
    )


def normalize_ffprobe_stream(stream, output_index):
    codec_type = stream.get("codec_type", "")
    out = {
        "index": output_index,
        "codec_type": codec_type,
        "codec_name": stream.get("codec_name", "unknown"),
    }
    if codec_type == "audio":
        codec_name = stream.get("codec_name", "unknown")
        out.update(
            {
                "sample_rate": int(stream.get("sample_rate", 0)),
                "channels": int(stream.get("channels", 0)),
                "bits_per_sample": int(
                    stream.get("bits_per_sample")
                    or stream.get("bits_per_raw_sample")
                    or bits_from_codec_name(codec_name)
                    or 0
                ),
                "duration_seconds": stream_duration(stream),
            }
        )
    elif codec_type == "video":
        out.update(
            {
                "width": int(stream.get("width", 0)),
                "height": int(stream.get("height", 0)),
                "duration_seconds": stream_duration(stream),
            }
        )
    return out


def normalize_format_name(format_name):
    names = set(str(format_name).split(","))
    if "wav" in names:
        return "wav"
    if "mp3" in names:
        return "mp3"
    if "flac" in names:
        return "flac"
    if "ogg" in names:
        return "ogg"
    if names.intersection({"mov", "mp4", "m4a", "3gp", "3g2", "mj2"}):
        return "mp4"
    return str(format_name).split(",")[0] if format_name else "unknown"


def stream_duration(stream):
    value = stream.get("duration")
    if value is None:
        return 0.0
    return round(float(value), 6)


def bits_from_codec_name(codec_name):
    if codec_name == "pcm_s16le":
        return 16
    if codec_name == "pcm_u8":
        return 8
    return 0


def normalize_probe_document(document):
    streams = document.get("streams") or []
    return {
        "format": normalize_format_name(document.get("format", "")),
        "streams": [normalize_probe_stream(stream, index) for index, stream in enumerate(streams)],
    }


def normalize_probe_stream(stream, output_index):
    codec_type = str(stream.get("codec_type", ""))
    out = {
        "index": output_index,
        "codec_type": codec_type,
        "codec_name": str(stream.get("codec_name", "")),
    }
    if codec_type == "audio":
        out.update(
            {
                "sample_rate": int(stream.get("sample_rate", 0)),
                "channels": int(stream.get("channels", 0)),
                "bits_per_sample": int(stream.get("bits_per_sample", 0)),
                "duration_seconds": round(float(stream.get("duration_seconds", 0.0)), 6),
            }
        )
    elif codec_type == "video":
        out.update(
            {
                "width": int(stream.get("width", 0)),
                "height": int(stream.get("height", 0)),
                "duration_seconds": round(float(stream.get("duration_seconds", 0.0)), 6),
            }
        )
    return out


def compare_probe(expected, actual):
    expected = normalize_probe_document(expected)
    actual = normalize_probe_document(actual)
    differences = []
    compare_value(expected["format"], actual["format"], "format", differences)
    compare_value(len(expected["streams"]), len(actual["streams"]), "stream count", differences)
    for index, (expected_stream, actual_stream) in enumerate(
        zip(expected["streams"], actual["streams"])
    ):
        compare_stream(expected_stream, actual_stream, f"stream {index}", differences)
    if not differences:
        return True, ""
    return False, "; ".join(differences)


def compare_stream(expected, actual, prefix, differences):
    for key, expected_value in expected.items():
        if key == "duration_seconds":
            actual_value = actual.get(key)
            tolerance = duration_tolerance(expected)
            if actual_value is None or abs(float(expected_value) - float(actual_value)) > tolerance:
                differences.append(
                    f"{prefix}.{key}: expected {expected_value}, got {actual_value}"
                )
        else:
            compare_value(expected_value, actual.get(key), f"{prefix}.{key}", differences)


def compare_value(expected, actual, label, differences):
    if expected != actual:
        differences.append(f"{label}: expected {expected}, got {actual}")


def duration_tolerance(stream):
    if stream.get("codec_type") == "audio" and stream.get("codec_name") in {"aac", "mp3", "opus"}:
        return COMPRESSED_AUDIO_DURATION_TOLERANCE_SECONDS
    return DEFAULT_DURATION_TOLERANCE_SECONDS


def main():
    if len(sys.argv) != 3:
        print("usage: compare_probe_json.py <expected.json> <actual.json>", file=sys.stderr)
        return 2
    expected = json.loads(Path(sys.argv[1]).read_text())
    actual = json.loads(Path(sys.argv[2]).read_text())
    ok, details = compare_probe(expected, actual)
    if ok:
        print("passed")
        return 0
    print(details)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
