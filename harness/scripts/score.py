#!/usr/bin/env python3
import json
import sys
from pathlib import Path

WEIGHTS = {
    "passed": 10,
    "failed": -20,
    "error": -20,
    "skipped": -10,
}


def main():
    if len(sys.argv) != 2:
        print("usage: score.py <correctness.json>", file=sys.stderr)
        return 2
    try:
        document = json.loads(Path(sys.argv[1]).read_text())
        tests = document["tests"]
        score = 0
        for test in tests:
            status = test["status"]
            if status not in WEIGHTS:
                raise ValueError(f"unknown test status: {status}")
            score += WEIGHTS[status]
    except Exception as error:
        print(f"malformed input: {error}", file=sys.stderr)
        return 2
    print(score)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
