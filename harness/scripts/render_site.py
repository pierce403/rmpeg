#!/usr/bin/env python3
import html
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TEMPLATE = ROOT / "site" / "templates" / "index.html"
DIST = ROOT / "site" / "dist"
CORRECTNESS = ROOT / "site" / "data" / "correctness.json"
BENCH_SUMMARY = ROOT / "site" / "data" / "benchmark-summary.json"
RUNS = ROOT / "agents" / "runs"


def main():
    correctness = read_json(CORRECTNESS, empty_correctness())
    benchmark_summary = read_json(BENCH_SUMMARY, empty_benchmarks())
    rendered = TEMPLATE.read_text()
    replacements = {
        "generated_at": escape(correctness.get("generated_at", "unknown")),
        "current_status": render_current_status(correctness),
        "correctness": render_correctness(correctness),
        "benchmarks": render_benchmarks(benchmark_summary),
        "known_failures": render_known_failures(correctness),
        "autoresearch_log": render_autoresearch_log(),
    }
    for key, value in replacements.items():
        rendered = rendered.replace("{{" + key + "}}", value)
    DIST.mkdir(parents=True, exist_ok=True)
    (DIST / "index.html").write_text(rendered)
    print(f"wrote {DIST / 'index.html'}")


def read_json(path, fallback):
    if path.exists():
        return json.loads(path.read_text())
    return fallback


def empty_correctness():
    return {
        "generated_at": "unknown",
        "rmpeg_commit": "unknown",
        "ffmpeg_version": "unknown",
        "summary": {"total": 0, "passed": 0, "failed": 0, "skipped": 0},
        "tests": [],
    }


def empty_benchmarks():
    return {"generated_at": "unknown", "benchmarks": []}


def render_current_status(correctness):
    tests = correctness.get("tests", [])
    probe = [test for test in tests if test.get("kind") == "probe-json"]
    decode = [test for test in tests if test.get("kind") == "framemd5"]
    rows = [
        ("WAV/PCM metadata", ratio(probe), status_class(probe)),
        ("WAV/PCM decode/hash", ratio(decode), status_class(decode)),
        ("MP3", "not started", "not-started"),
        ("MP4/MOV", "not started", "not-started"),
        ("H.264", "not started", "not-started"),
        ("Filters", "not started", "not-started"),
    ]
    return table(["Area", "Status"], rows)


def ratio(tests):
    if not tests:
        return "0/0"
    passed = sum(1 for test in tests if test.get("status") == "passed")
    return f"{passed}/{len(tests)} passing"


def status_class(tests):
    if not tests:
        return "not-started"
    if all(test.get("status") == "passed" for test in tests):
        return "passed"
    return "failed"


def render_correctness(correctness):
    rows = []
    for test in correctness.get("tests", []):
        rows.append(
            (
                escape(test.get("name", "")),
                escape(test.get("kind", "")),
                badge(test.get("status", "unknown")),
                escape(test.get("details", "")),
            )
        )
    if not rows:
        rows.append(("No mirrored tests have been run.", "", badge("skipped"), ""))
    return table(["Test", "Kind", "Status", "Details"], rows)


def render_benchmarks(summary):
    rows = []
    for bench in summary.get("benchmarks", []):
        rows.append(
            (
                escape(bench.get("name", "")),
                seconds(bench.get("ffmpeg_seconds_mean", 0.0)),
                seconds(bench.get("rmpeg_seconds_mean", 0.0)),
                escape(bench.get("relative", "")),
                badge(bench.get("status", "unknown")),
            )
        )
    if not rows:
        rows.append(("No benchmarks have been run.", "", "", "not measured", badge("skipped")))
    return table(["Benchmark", "FFmpeg mean", "rmpeg mean", "Relative", "Status"], rows)


def render_known_failures(correctness):
    failures = [
        test
        for test in correctness.get("tests", [])
        if test.get("status") in {"failed", "error", "skipped"}
    ]
    if not failures:
        return "<p>No mirrored failures in the current WAV/PCM slice. Other formats are not implemented.</p>"
    items = []
    for test in failures:
        items.append(
            f"<li><strong>{escape(test.get('name', ''))}</strong>: "
            f"{badge(test.get('status', 'unknown'))} {escape(test.get('details', ''))}</li>"
        )
    return "<ul>" + "\n".join(items) + "</ul>"


def render_autoresearch_log():
    logs = sorted(RUNS.glob("*.md"), reverse=True)[:5] if RUNS.exists() else []
    if not logs:
        return "<p>No autoresearch runs recorded yet.</p>"
    items = []
    for path in logs:
        first_line = path.read_text().splitlines()[0] if path.read_text().splitlines() else path.name
        items.append(f"<li><code>{escape(path.name)}</code> {escape(first_line)}</li>")
    return "<ul>" + "\n".join(items) + "</ul>"


def table(headers, rows):
    head = "".join(f"<th>{escape(header)}</th>" for header in headers)
    body_rows = []
    for row in rows:
        row_class = ""
        if row and isinstance(row[-1], str) and row[-1] in {"passed", "failed", "not-started"}:
            row_class = f' class="{row[-1]}"'
            row = row[:-1]
        cells = "".join(f"<td>{cell}</td>" for cell in row)
        body_rows.append(f"<tr{row_class}>{cells}</tr>")
    return f"<table><thead><tr>{head}</tr></thead><tbody>{''.join(body_rows)}</tbody></table>"


def badge(status):
    safe = escape(status)
    css = "failed" if status in {"failed", "error"} else status
    return f'<span class="badge {escape(css)}">{safe}</span>'


def seconds(value):
    try:
        return f"{float(value):.6f}s"
    except (TypeError, ValueError):
        return "unknown"


def escape(value):
    return html.escape(str(value), quote=True)


if __name__ == "__main__":
    main()
