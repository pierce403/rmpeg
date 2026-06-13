#!/usr/bin/env python3
import argparse
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
    for sample in samples:
        write_probe_reference(sample)
        write_framemd5_reference(sample)
    print(f"generated references in {REFERENCE}")
    return 0


def require_tool(name):
    if shutil.which(name) is None:
        raise SystemExit(f"required tool missing: {name}")


def sample_paths():
    return sorted(path for path in SAMPLES.glob("*.wav") if path.is_file())


def stem(path):
    return path.name.replace(".wav", "")


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


def write_framemd5_reference(sample):
    result = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", str(sample), "-f", "framemd5", "-"],
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
    write_json(REFERENCE / f"{stem(sample)}.framemd5.json", output)


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
        tests.append(run_framemd5_test(rmpeg, sample))

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
    name = f"probe wav {stem(sample)}"
    ref_path = REFERENCE / f"{stem(sample)}.probe.json"
    if not ref_path.exists():
        return skipped(name, "probe-json", f"missing reference {ref_path}")
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


def run_framemd5_test(rmpeg, sample):
    name = f"decode/hash wav {stem(sample)}"
    ref_path = REFERENCE / f"{stem(sample)}.framemd5.json"
    if not ref_path.exists():
        return skipped(name, "framemd5", f"missing reference {ref_path}")
    ref = json.loads(ref_path.read_text())
    result = subprocess.run(
        [str(rmpeg), "decode", str(sample), "--framemd5"],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if ref.get("expected_error"):
        if result.returncode != 0:
            return passed(name, "framemd5", "both ffmpeg and rmpeg rejected input")
        return failed(name, "framemd5", "ffmpeg rejected input but rmpeg accepted it")
    if result.returncode != 0:
        return failed(name, "framemd5", trim(result.stderr))
    try:
        ok, details = compare_framemd5(ref["frames"], result.stdout)
    except Exception as error:
        return errored(name, "framemd5", str(error))
    return passed(name, "framemd5") if ok else failed(name, "framemd5", details)


def passed(name, kind, details=""):
    return {"name": name, "kind": kind, "status": "passed", "details": details}


def failed(name, kind, details):
    return {"name": name, "kind": kind, "status": "failed", "details": details}


def skipped(name, kind, details):
    return {"name": name, "kind": kind, "status": "skipped", "details": details}


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
