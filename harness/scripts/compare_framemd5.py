#!/usr/bin/env python3
import json
import sys
from pathlib import Path


def parse_framemd5_text(text):
    frames = []
    for raw_line in text.splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        parts = [part.strip() for part in line.split(",")]
        if len(parts) != 6:
            raise ValueError(f"invalid framemd5 row: {raw_line}")
        frames.append(
            {
                "stream_index": int(parts[0]),
                "dts": int(parts[1]),
                "pts": int(parts[2]),
                "duration": int(parts[3]),
                "size": int(parts[4]),
                "hash": parts[5],
            }
        )
    return frames


def compare_framemd5(expected_frames, actual_text):
    actual_frames = parse_framemd5_text(actual_text)
    if expected_frames == actual_frames:
        return True, ""
    if len(expected_frames) != len(actual_frames):
        return (
            False,
            f"frame count differs: expected {len(expected_frames)}, got {len(actual_frames)}",
        )
    for index, (expected, actual) in enumerate(zip(expected_frames, actual_frames)):
        if expected != actual:
            return False, f"frame {index} differs: expected {expected}, got {actual}"
    return False, "framemd5 differs"


def main():
    if len(sys.argv) != 3:
        print("usage: compare_framemd5.py <expected.json> <actual.txt>", file=sys.stderr)
        return 2
    expected = json.loads(Path(sys.argv[1]).read_text())
    actual_text = Path(sys.argv[2]).read_text()
    ok, details = compare_framemd5(expected["frames"], actual_text)
    if ok:
        print("passed")
        return 0
    print(details)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
