#!/usr/bin/env python3
import json
import sys
from pathlib import Path


def normalize_ffprobe_document(document):
    streams = document.get("streams", [])
    audio = next((stream for stream in streams if stream.get("codec_type") == "audio"), None)
    if audio is None:
        raise ValueError("ffprobe output has no audio stream")

    duration = audio.get("duration") or document.get("format", {}).get("duration") or 0
    codec_name = audio.get("codec_name", "unknown")
    bits_per_sample = (
        audio.get("bits_per_sample")
        or audio.get("bits_per_raw_sample")
        or bits_from_codec_name(codec_name)
        or 0
    )
    return normalize_probe_document(
        {
            "format": "wav",
            "streams": [
                {
                    "index": int(audio.get("index", 0)),
                    "codec_type": "audio",
                    "codec_name": codec_name,
                    "sample_rate": int(audio.get("sample_rate", 0)),
                    "channels": int(audio.get("channels", 0)),
                    "bits_per_sample": int(bits_per_sample),
                    "duration_seconds": float(duration),
                }
            ],
        }
    )


def bits_from_codec_name(codec_name):
    if codec_name == "pcm_s16le":
        return 16
    if codec_name == "pcm_u8":
        return 8
    return None


def normalize_probe_document(document):
    streams = document.get("streams") or []
    if len(streams) != 1:
        raise ValueError(f"expected exactly one stream, got {len(streams)}")
    stream = streams[0]
    return {
        "format": document.get("format"),
        "streams": [
            {
                "index": int(stream.get("index", 0)),
                "codec_type": str(stream.get("codec_type", "")),
                "codec_name": str(stream.get("codec_name", "")),
                "sample_rate": int(stream.get("sample_rate", 0)),
                "channels": int(stream.get("channels", 0)),
                "bits_per_sample": int(stream.get("bits_per_sample", 0)),
                "duration_seconds": round(float(stream.get("duration_seconds", 0.0)), 6),
            }
        ],
    }


def compare_probe(expected, actual):
    expected = normalize_probe_document(expected)
    actual = normalize_probe_document(actual)
    if expected == actual:
        return True, ""
    return False, f"expected {json.dumps(expected, sort_keys=True)}, got {json.dumps(actual, sort_keys=True)}"


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
