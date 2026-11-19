#!/usr/bin/env python3
"""Convert nextest's `libtest-json-plus` output into JUnit XML.

nextest 0.9.x no longer writes JUnit XML itself (`--junit-xml` was removed); the
supported machine-readable path is the experimental libtest JSON stream. This
script turns that stream into a standard JUnit XML file so CI can archive it and
the Pages workflow can render it.

Usage:
    NEXTEST_EXPERIMENTAL_LIBTEST_JSON=1 \
        cargo nextest run --workspace --message-format libtest-json-plus > run.ndjson
    python scripts/nextest_junit.py run.ndjson -o junit.xml --name "nextest (linux)"

Empty or partially written NDJSON (build failure, cancelled run) still produces a
valid, empty <testsuites> document instead of raising.
"""

from __future__ import annotations

import argparse
import json
import sys
import xml.etree.ElementTree as ET
from pathlib import Path


def _split_name(full: str, fallback: str) -> tuple[str, str]:
    """`crate::binary$module::test` -> (`crate::binary`, `module::test`)."""
    classname, sep, test = full.partition("$")
    if not sep:
        return fallback, full
    return classname, test


def _read_events(paths: list[Path]) -> list[dict]:
    events: list[dict] = []
    for path in paths:
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError as exc:  # pragma: no cover - CI reports the message instead
            print(f"warning: cannot read {path}: {exc}", file=sys.stderr)
            continue
        for lineno, line in enumerate(text.splitlines(), 1):
            line = line.strip()
            if not line:
                continue
            try:
                events.append(json.loads(line))
            except json.JSONDecodeError:
                print(
                    f"warning: {path}:{lineno}: skipping malformed JSON line",
                    file=sys.stderr,
                )
    return events


def _collect(events: list[dict]) -> dict[tuple[str, str, str], dict]:
    """Group test events by (crate, test binary, kind), preserving first-seen order.

    `suite` events carry the crate/binary/kind metadata, while `test` events only
    carry a `crate::binary$test` name. A binary is looked up through `by_binary`
    so test events join the suite opened by the matching `suite` event instead of
    creating a duplicate suite keyed by the unknown kind.
    """
    suites: dict[tuple[str, str, str], dict] = {}
    by_binary: dict[tuple[str, str], tuple[str, str, str]] = {}

    def new_suite(crate: str, binary: str, kind: str) -> tuple[tuple[str, str, str], dict]:
        key = (crate, binary, kind)
        suite = {
            "crate": crate,
            "binary": binary,
            "kind": kind,
            "cases": {},
            "summary": None,
        }
        suites[key] = suite
        by_binary[(crate, binary)] = key
        return key, suite

    for event in events:
        kind = event.get("type")
        if kind == "suite":
            meta = event.get("nextest") or {}
            crate = meta.get("crate")
            binary = meta.get("test_binary")
            if not crate or not binary:
                continue
            key = by_binary.get((crate, binary))
            if key is None:
                key, suite = new_suite(crate, binary, meta.get("kind") or "test")
                suite["kind"] = meta.get("kind") or suite["kind"]
            suite = suites[key]
            if event.get("event") in {"ok", "failed"}:
                suite["summary"] = event
        elif kind == "test":
            name = event.get("name")
            if not name:
                continue
            crate, _, rest = name.partition("::")
            binary, _, _ = rest.partition("$")
            key = by_binary.get((crate, binary))
            if key is None:
                key, _ = new_suite(crate, binary, "test")
            suites[key]["cases"][name] = event

    return suites


def _case_status(event: dict) -> str:
    return event.get("event") or "unknown"


def _build_xml(suites: dict, name: str, timestamp: str | None) -> ET.ElementTree:
    total_tests = 0
    total_failures = 0
    total_errors = 0
    total_skipped = 0
    total_time = 0.0
    suite_nodes: list[ET.Element] = []

    for suite in suites.values():
        summary = suite["summary"] or {}
        cases = suite["cases"]
        passed = int(summary.get("passed", 0))
        failed = int(summary.get("failed", 0))
        ignored = int(summary.get("ignored", 0))
        measured = int(summary.get("measured", 0))
        if summary:
            tests = passed + failed + ignored + measured
            time = float(summary.get("exec_time") or 0.0)
        else:
            tests = len(cases)
            failed = sum(1 for e in cases.values() if _case_status(e) == "failed")
            ignored = sum(
                1 for e in cases.values() if _case_status(e) in {"ignored", "measured"}
            )
            time = sum(float(e.get("exec_time") or 0.0) for e in cases.values())

        skipped = ignored + measured
        suite_name = f"{suite['crate']} ({suite['binary']}, {suite['kind']})"
        node = ET.Element(
            "testsuite",
            {
                "name": suite_name,
                "tests": str(tests),
                "failures": str(failed),
                "errors": "0",
                "skipped": str(skipped),
                "time": f"{time:.3f}",
            },
        )
        props = ET.SubElement(node, "properties")
        ET.SubElement(props, "property", {"name": "crate", "value": suite["crate"]})
        ET.SubElement(props, "property", {"name": "test_binary", "value": suite["binary"]})
        ET.SubElement(props, "property", {"name": "kind", "value": suite["kind"]})

        for full_name in sorted(cases):
            event = cases[full_name]
            status = _case_status(event)
            classname, test_name = _split_name(full_name, suite_name)
            case = ET.SubElement(
                node,
                "testcase",
                {
                    "classname": classname,
                    "name": test_name,
                    "time": f"{float(event.get('exec_time') or 0.0):.3f}",
                },
            )
            stdout = event.get("stdout") or ""
            message = event.get("message") or ""
            if status == "failed":
                failure = ET.SubElement(
                    case,
                    "failure",
                    {"message": message or "test failed", "type": "assertion"},
                )
                if stdout:
                    failure.text = stdout
            elif status == "timeout":
                failure = ET.SubElement(
                    case,
                    "failure",
                    {"message": message or "test timed out", "type": "timeout"},
                )
                if stdout:
                    failure.text = stdout
            elif status in {"ignored", "measured"}:
                ET.SubElement(case, "skipped")
            if stdout and status != "failed":
                ET.SubElement(case, "system-out").text = stdout

        suite_nodes.append(node)
        total_tests += tests
        total_failures += failed
        total_skipped += skipped
        total_time += time

    attrs = {
        "name": name,
        "tests": str(total_tests),
        "failures": str(total_failures),
        "errors": str(total_errors),
        "skipped": str(total_skipped),
        "time": f"{total_time:.3f}",
    }
    if timestamp:
        attrs["timestamp"] = timestamp
    root = ET.Element("testsuites", attrs)
    root.extend(suite_nodes)
    tree = ET.ElementTree(root)
    ET.indent(tree, space="  ")
    return tree


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("inputs", nargs="+", type=Path, help="nextest NDJSON file(s)")
    parser.add_argument("-o", "--output", required=True, type=Path, help="JUnit XML path")
    parser.add_argument("--name", default="nextest", help="root testsuites name")
    parser.add_argument(
        "--timestamp",
        default=None,
        help="optional ISO-8601 timestamp recorded on the testsuites element",
    )
    args = parser.parse_args()

    suites = _collect(_read_events(args.inputs))
    tree = _build_xml(suites, args.name, args.timestamp)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    tree.write(args.output, encoding="utf-8", xml_declaration=True)

    tests = sum(int(node.get("tests", 0)) for node in tree.getroot())
    failures = sum(int(node.get("failures", 0)) for node in tree.getroot())
    print(
        f"wrote {args.output} ({len(suites)} suites, {tests} tests, {failures} failures)",
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
