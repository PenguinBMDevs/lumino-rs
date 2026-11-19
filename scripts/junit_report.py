#!/usr/bin/env python3
"""Render JUnit XML files (produced by `scripts/nextest_junit.py`) as a static site.

The output is a single self-contained `index.html` with inline CSS/JS plus copies of
the raw XML files, meant to be published through GitHub Pages.

Usage:
    python scripts/junit_report.py reports -o site \
        --title "Lumino Test Report" --run-url "$CI_RUN_URL" ...
"""

from __future__ import annotations

import argparse
import html
import os
import shutil
import sys
import xml.etree.ElementTree as ET
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path


@dataclass
class Case:
    classname: str
    name: str
    time: float
    status: str = "passed"
    message: str = ""
    stdout: str = ""


@dataclass
class Suite:
    name: str
    tests: int
    failures: int
    errors: int
    skipped: int
    time: float
    cases: list[Case] = field(default_factory=list)


@dataclass
class Report:
    label: str
    source: Path
    suites: list[Suite] = field(default_factory=list)

    @property
    def tests(self) -> int:
        return sum(s.tests for s in self.suites)

    @property
    def failures(self) -> int:
        return sum(s.failures + s.errors for s in self.suites)

    @property
    def skipped(self) -> int:
        return sum(s.skipped for s in self.suites)

    @property
    def time(self) -> float:
        return sum(s.time for s in self.suites)


def _parse_report(path: Path) -> Report:
    label = path.stem
    for prefix in ("junit-", "junit_"):
        if label.startswith(prefix):
            label = label[len(prefix) :]
    report = Report(label=label or path.parent.name, source=path)
    try:
        root = ET.parse(path).getroot()
    except ET.ParseError as exc:
        print(f"warning: skipping unparsable {path}: {exc}", file=sys.stderr)
        return report

    suites = [root] if root.tag == "testsuite" else root.findall("testsuite")
    for node in suites:
        suite = Suite(
            name=node.get("name") or "(unnamed suite)",
            tests=int(node.get("tests") or 0),
            failures=int(node.get("failures") or 0),
            errors=int(node.get("errors") or 0),
            skipped=int(node.get("skipped") or 0),
            time=float(node.get("time") or 0.0),
        )
        for case in node.findall("testcase"):
            item = Case(
                classname=case.get("classname") or "",
                name=case.get("name") or "",
                time=float(case.get("time") or 0.0),
            )
            failure = case.find("failure")
            error = case.find("error")
            skipped = case.find("skipped")
            if failure is not None or error is not None:
                node_fail = failure if failure is not None else error
                item.status = "failed"
                item.message = node_fail.get("message") or "failed"
                item.stdout = node_fail.text or ""
            elif skipped is not None:
                item.status = "skipped"
            suite.cases.append(item)
        report.suites.append(suite)
    return report


def _fmt_time(seconds: float) -> str:
    if seconds < 1:
        return f"{seconds * 1000:.0f} ms"
    if seconds < 60:
        return f"{seconds:.2f} s"
    minutes, rest = divmod(seconds, 60)
    return f"{int(minutes)}m {rest:.0f}s"


def _pill(status: str) -> str:
    return f'<span class="pill {html.escape(status)}">{html.escape(status)}</span>'


def _render_case_row(report: Report, suite: Suite, case: Case) -> str:
    haystack = f"{report.label} {suite.name} {case.classname} {case.name}".lower()
    detail = ""
    if case.status == "failed":
        detail = "<pre class=\"trace\">" + html.escape(
            (case.message + "\n" + case.stdout).strip()
        ) + "</pre>"
    return (
        f'<tr data-find="{html.escape(haystack)}" class="row-{case.status}">'
        f'<td class="status">{_pill(case.status)}</td>'
        f'<td class="mono">{html.escape(case.name)}</td>'
        f'<td class="num">{_fmt_time(case.time)}</td>'
        f"</tr>"
        + (f'<tr class="detail-row"><td colspan="3">{detail}</td></tr>' if detail else "")
    )


def _render_suite(report: Report, suite: Suite) -> str:
    failures = suite.failures + suite.errors
    state = "failed" if failures else "passed"
    cases = ""
    if suite.cases:
        rows = "".join(_render_case_row(report, suite, c) for c in suite.cases)
        cases = (
            '<table class="cases"><thead><tr><th>Status</th><th>Test</th>'
            "<th>Time</th></tr></thead><tbody>"
            f"{rows}</tbody></table>"
        )
    return (
        f'<details class="suite {state}"{" open" if failures else ""}>'
        f"<summary><span class=\"mono\">{html.escape(suite.name)}</span>"
        f"<span class=\"counts\">{suite.tests} tests · {failures} failed · "
        f"{suite.skipped} skipped · {_fmt_time(suite.time)}</span></summary>"
        f"{cases}</details>"
    )


def _render_report(report: Report) -> str:
    failures = report.failures
    state = "failed" if failures else "passed"
    suites = "".join(_render_suite(report, s) for s in report.suites)
    return (
        f'<section class="report {state}">'
        f'<header class="report-head"><h2>{html.escape(report.label)}</h2>'
        f'<span class="pill {state}">{"FAILED" if failures else "PASSED"}</span>'
        f'<span class="counts">{report.tests} tests · {failures} failed · '
        f"{report.skipped} skipped · {_fmt_time(report.time)}</span></header>"
        f"{suites or '<p class=\"muted\">No test suites in this report.</p>'}"
        "</section>"
    )


def _render_failures(reports: list[Report]) -> str:
    items: list[str] = []
    for report in reports:
        for suite in report.suites:
            for case in suite.cases:
                if case.status != "failed":
                    continue
                trace = (case.message + "\n" + case.stdout).strip()
                items.append(
                    "<details class=\"failure\" open>"
                    f"<summary><strong>{html.escape(case.name)}</strong>"
                    f"<span class=\"muted\"> — {html.escape(report.label)} / "
                    f"{html.escape(suite.name)}</span></summary>"
                    f"<pre class=\"trace\">{html.escape(trace)}</pre></details>"
                )
    if not items:
        return '<section class="failures"><h2>Failures</h2><p class="ok">None</p></section>'
    return (
        '<section class="failures"><h2>Failures '
        f"({len(items)})</h2>{''.join(items)}</section>"
    )


def _render_page(reports: list[Report], meta: dict[str, str]) -> str:
    total_tests = sum(r.tests for r in reports)
    total_failures = sum(r.failures for r in reports)
    total_skipped = sum(r.skipped for r in reports)
    total_time = sum(r.time for r in reports)
    conclusion = meta.get("conclusion") or ("failure" if total_failures else "success")
    commit = meta.get("sha") or ""
    short_sha = commit[:8]
    run_url = meta.get("run_url") or ""
    generated = meta.get("generated") or datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M UTC")

    run_link = (
        f'<a href="{html.escape(run_url)}">{html.escape(meta.get("run_id") or "CI run")}</a>'
        if run_url
        else html.escape(meta.get("run_id") or "local run")
    )
    commit_html = f'<code>{html.escape(short_sha)}</code>' if commit else ""
    platforms = "".join(_render_report(r) for r in reports) or (
        '<p class="muted">No JUnit reports were downloaded for this run.</p>'
    )
    raw_links = " · ".join(
        f'<a href="data/{html.escape(r.source.name)}">{html.escape(r.label)}</a>' for r in reports
    )
    empty = "empty" if not reports else ""

    return f"""<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{html.escape(meta.get("title") or "Test Report")}</title>
<style>
:root {{ color-scheme: dark; }}
* {{ box-sizing: border-box; }}
body {{ margin: 0; font: 14px/1.5 system-ui, "Segoe UI", sans-serif;
  background: #12161f; color: #d7dce5; }}
a {{ color: #7cc4ff; }}
header.page, main {{ max-width: 1200px; margin: 0 auto; padding: 0 20px; }}
header.page {{ padding-top: 28px; }}
h1 {{ font-size: 22px; margin: 0 0 6px; }}
h2 {{ font-size: 17px; margin: 22px 0 10px; }}
.meta {{ color: #8b94a6; font-size: 13px; }}
.cards {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
  gap: 12px; margin: 20px 0; }}
.card {{ background: #1a2130; border: 1px solid #253048; border-radius: 10px;
  padding: 14px 16px; }}
.card .num {{ display: block; font-size: 24px; font-weight: 650; }}
.card .label {{ color: #8b94a6; font-size: 12px; text-transform: uppercase;
  letter-spacing: .06em; }}
.pill {{ display: inline-block; padding: 1px 8px; border-radius: 999px;
  font-size: 11px; font-weight: 600; text-transform: uppercase; }}
.pill.passed {{ background: #123524; color: #5ee59b; }}
.pill.failed {{ background: #3a1519; color: #ff8189; }}
.pill.skipped {{ background: #2a2f3a; color: #aab4c6; }}
.report {{ background: #161c28; border: 1px solid #253048; border-radius: 10px;
  margin: 14px 0; padding: 12px 16px; }}
.report.failed {{ border-color: #7a2630; }}
.report-head {{ display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }}
.report-head h2 {{ margin: 0; font-size: 16px; }}
.counts {{ color: #8b94a6; font-size: 12px; }}
details.suite {{ border-top: 1px solid #222b3d; padding: 6px 0; }}
details.suite:first-of-type {{ border-top: 0; }}
details.suite > summary {{ cursor: pointer; display: flex; justify-content: space-between;
  gap: 12px; padding: 4px 0; }}
details.suite.failed > summary {{ color: #ff9aa1; }}
.mono {{ font-family: ui-monospace, "Cascadia Mono", Consolas, monospace; font-size: 12.5px; }}
table.cases {{ width: 100%; border-collapse: collapse; margin: 8px 0 12px; }}
table.cases th {{ text-align: left; color: #8b94a6; font-size: 12px; font-weight: 500;
  padding: 4px 8px; border-bottom: 1px solid #253048; }}
table.cases td {{ padding: 4px 8px; border-bottom: 1px solid #1c2331;
  vertical-align: top; }}
tr.row-failed td {{ background: #241317; }}
tr.row-skipped td {{ color: #8b94a6; }}
td.num {{ text-align: right; white-space: nowrap; color: #8b94a6; }}
pre.trace {{ margin: 6px 0 10px; padding: 10px; background: #0d1118; border: 1px solid #253048;
  border-radius: 8px; overflow: auto; max-height: 320px; color: #ffb3b9; font-size: 12px; }}
details.failure {{ background: #1a1216; border: 1px solid #5a2028; border-radius: 8px;
  padding: 8px 12px; margin: 8px 0; }}
details.failure > summary {{ cursor: pointer; }}
.search {{ margin: 12px 0; }}
.search input {{ width: 100%; max-width: 420px; padding: 8px 10px; border-radius: 8px;
  border: 1px solid #2b3854; background: #0f141d; color: inherit; }}
.muted {{ color: #8b94a6; }}
.ok {{ color: #5ee59b; }}
footer {{ color: #6f7a8d; font-size: 12px; padding: 24px 20px 40px; text-align: center; }}
body.filtered tr.detail-row {{ display: none; }}
</style>
</head>
<body class="{empty}">
<header class="page">
  <h1>{html.escape(meta.get("title") or "Test Report")}</h1>
  <p class="meta">
    {_pill(conclusion)} run {run_link} {commit_html} · generated {html.escape(generated)}
  </p>
  <div class="cards">
    <div class="card"><span class="num">{total_tests}</span><span class="label">tests</span></div>
    <div class="card"><span class="num">{total_tests - total_failures - total_skipped}</span><span class="label">passed</span></div>
    <div class="card"><span class="num">{total_failures}</span><span class="label">failed</span></div>
    <div class="card"><span class="num">{total_skipped}</span><span class="label">skipped</span></div>
    <div class="card"><span class="num">{_fmt_time(total_time)}</span><span class="label">duration</span></div>
    <div class="card"><span class="num">{len(reports)}</span><span class="label">platforms</span></div>
  </div>
  <div class="search"><input id="q" type="search"
    placeholder="Filter by platform / suite / test name…" autocomplete="off"></div>
</header>
<main>
  {_render_failures(reports)}
  <section class="platforms"><h2>Platforms</h2>{platforms}</section>
</main>
<footer>Raw JUnit XML: {raw_links or "none"} · lumino-rs CI</footer>
<script>
const q = document.getElementById('q');
q.addEventListener('input', () => {{
  const needle = q.value.trim().toLowerCase();
  document.body.classList.toggle('filtered', needle.length > 0);
  for (const row of document.querySelectorAll('tr[data-find]')) {{
    row.style.display = needle && !row.dataset.find.includes(needle) ? 'none' : '';
  }}
  for (const details of document.querySelectorAll('details.suite')) {{
    if (needle) details.open = true;
  }}
}});
</script>
</body>
</html>
"""


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("inputs", nargs="+", type=Path, help="JUnit XML files or directories")
    parser.add_argument("-o", "--output", required=True, type=Path, help="output directory")
    parser.add_argument("--title", default="Lumino Test Report")
    parser.add_argument("--run-url", default=os.environ.get("CI_RUN_URL", ""))
    parser.add_argument("--run-id", default=os.environ.get("CI_RUN_ID", ""))
    parser.add_argument("--sha", default=os.environ.get("CI_SHA", ""))
    parser.add_argument("--conclusion", default=os.environ.get("CI_CONCLUSION", ""))
    parser.add_argument("--generated", default=os.environ.get("CI_GENERATED", ""))
    args = parser.parse_args()

    xml_paths: list[Path] = []
    for item in args.inputs:
        if item.is_dir():
            xml_paths.extend(sorted(item.rglob("*.xml")))
        elif item.suffix.lower() == ".xml":
            xml_paths.append(item)
    reports = [_parse_report(path) for path in xml_paths]
    reports.sort(key=lambda r: r.label)

    args.output.mkdir(parents=True, exist_ok=True)
    data_dir = args.output / "data"
    data_dir.mkdir(exist_ok=True)
    for report in reports:
        shutil.copy2(report.source, data_dir / report.source.name)

    meta = {
        "title": args.title,
        "run_url": args.run_url,
        "run_id": args.run_id,
        "sha": args.sha,
        "conclusion": args.conclusion,
        "generated": args.generated,
    }
    index = args.output / "index.html"
    index.write_text(_render_page(reports, meta), encoding="utf-8")
    print(
        f"wrote {index} from {len(reports)} report(s), {sum(r.tests for r in reports)} tests",
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
