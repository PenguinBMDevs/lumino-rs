#!/usr/bin/env python3
r"""i18n 覆盖缺口审计工具。

扫描工作区全部 `.rs` 源码，找出**含中文字符串字面量**并按性质分类，
用于回答两个问题：

1. 还有多少硬编码中文没进 i18n？
2. 其中哪些是「用户可见文案」，哪些只是日志/测试/数据（不该进 i18n）？

## 分类口径

| 分类 | 含义 |
|---|---|
| `i18n-table` | i18n 表自身（`crates/*/i18n*`），不算缺口 |
| `i18n-wrapped` | 已被 `tr!` / `trf!` / `i18n::tr(...)` 包住的原文，已进 i18n |
| `log` | `tracing::*!` / `debug!` 等日志宏（含跨行调用），面向开发者 |
| `assert/panic` | `assert!` / `panic!` / `todo!` / `unreachable!` |
| `expect` | `.expect("…")` / `.unwrap_or_else(\|…\| …)` 兜底文案 |
| `thiserror` | `#[error("…")]` 属性宏 |
| `enum-parse` | 由 `impl_unit_enum_from_str!` 生成的解析错误串 |
| `print` | `println!` / `eprintln!` |
| `ui` | **其余** —— 疑似用户可见文案，缺口主体 |

测试代码单独统计（`tests/`、`benches/`、`examples/`、`#[cfg(test)]` 门控模块、
`tests_*.rs` / `*_tests.rs` / `*_tests/` 目录），不计入缺口。

## 用法

```bash
python scripts/i18n_audit.py                      # 控制台汇总
python scripts/i18n_audit.py --json out.json      # 逐条明细
python scripts/i18n_audit.py --buckets outdir     # 按目录域切工作清单
python scripts/i18n_audit.py --report docs/x.md   # markdown 报告
```
"""

from __future__ import annotations

import argparse
import collections
import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CJK = re.compile(r"[\u2e80-\u9fff\u3000-\u303f\uff00-\uffef]")
SKIP_DIRS = {"target", ".git", "node_modules", ".codegraph", ".agents", ".ai-context"}

MACRO_OPEN = re.compile(
    r"\b(?:tracing::)?(trace|debug|info|warn|error)!\s*\(|"
    r"\b(panic|unreachable|todo|unimplemented|assert|assert_eq|assert_ne|debug_assert|"
    r"debug_assert_eq|debug_assert_ne|println|eprintln|print)!\s*\(|"
    r"\.(expect|unwrap_or_else)\s*\(|"
    r"#\[error\s*\(|"
    r"impl_unit_enum_from_str\s*!"
)


# --------------------------------------------------------------------------- 文件遍历


def rs_files() -> list[str]:
    out: list[str] = []
    for dirpath, dirnames, filenames in os.walk(ROOT):
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
        for fn in filenames:
            if fn.endswith(".rs"):
                out.append(os.path.join(dirpath, fn))
    return sorted(out)


# --------------------------------------------------------------- cfg(test) 模块传播

MOD_DECL = re.compile(
    r"#\[cfg\(test\)\][\s\r\n]*(?:#\[[^\]]*\][\s\r\n]*)*"
    r"(?:#\[path\s*=\s*\"([^\"]+)\"\][\s\r\n]*)?"
    r"(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*[;{]"
)

TEST_DIR_HINTS = (
    "/benches/",
    "/examples/",
    "/tests/",
    "/_tests/",
    "/root_tests/",
    "/bend_tests/",
    "/depth_tests/",
    "/perf_tests/",
    "/document_tests/",
    "/control_tests/",
)


def module_dir_of(path: str) -> str:
    d = os.path.dirname(path)
    stem = os.path.splitext(os.path.basename(path))[0]
    cand = os.path.join(d, stem)
    return cand if os.path.isdir(cand) else d


class TestModules:
    """按模块图传播 `#[cfg(test)]`，判定哪些文件属于测试代码。"""

    def __init__(self, all_files: list[str]) -> None:
        self.test_files: set[str] = set()
        self.visited: set[str] = set()
        for f in all_files:
            rel = os.path.relpath(f, ROOT).replace("\\", "/")
            if rel.startswith(("examples/", "tests/", "benches/")) or any(
                h in "/" + rel for h in TEST_DIR_HINTS
            ):
                self.test_files.add(os.path.normpath(f))
        for f in all_files:
            rel = os.path.relpath(f, ROOT).replace("\\", "/")
            if os.path.basename(f) in ("lib.rs", "main.rs") and not rel.startswith(
                ("examples/", "tests/", "benches/")
            ):
                self._mark(f, False)
        for _ in range(8):
            before = len(self.visited)
            for f in all_files:
                rel = os.path.relpath(f, ROOT).replace("\\", "/")
                if os.path.basename(f) in ("lib.rs", "main.rs") and not rel.startswith(
                    ("examples/", "tests/", "benches/")
                ):
                    self._mark(f, False)
            if len(self.visited) == before:
                break

    def _mark(self, entry: str, is_test: bool) -> None:
        entry = os.path.normpath(entry)
        if entry in self.visited or not os.path.isfile(entry):
            return
        self.visited.add(entry)
        if is_test:
            self.test_files.add(entry)
        try:
            src = open(entry, encoding="utf-8").read()
        except UnicodeDecodeError:
            return
        base = module_dir_of(entry)
        for m in MOD_DECL.finditer(src):
            path_attr, name = m.group(1), m.group(2)
            targets = []
            if path_attr:
                targets.append(
                    os.path.normpath(
                        os.path.join(os.path.dirname(entry), path_attr.replace("\\", "/"))
                    )
                )
            targets.append(os.path.join(base, name + ".rs"))
            targets.append(os.path.join(base, name))
            for t in targets:
                if t.endswith(".rs") and os.path.isfile(t):
                    self._mark(t, True)
                elif os.path.isdir(t):
                    for dp, _dn, fns in os.walk(t):
                        for f in fns:
                            if f.endswith(".rs"):
                                self._mark(os.path.join(dp, f), True)

    def is_test(self, path: str) -> bool:
        p = os.path.normpath(path)
        rel = os.path.relpath(p, ROOT).replace("\\", "/")
        if p in self.test_files:
            return True
        if any(h in "/" + rel for h in TEST_DIR_HINTS):
            return True
        b = os.path.basename(rel)
        return (
            b.startswith("tests_")
            or b.endswith(("_tests.rs", "_test.rs"))
            or b in ("tests.rs", "test.rs")
        )


# --------------------------------------------------------------- 字面量提取与分类


def cfg_test_ranges(src: str) -> list[tuple[int, int]]:
    out = []
    for m in re.finditer(r"#\[cfg\(test\)\]", src):
        brace = src.find("{", m.end())
        if brace == -1:
            continue
        depth, i = 0, brace
        while i < len(src):
            if src[i] == "{":
                depth += 1
            elif src[i] == "}":
                depth -= 1
                if depth == 0:
                    break
            i += 1
        out.append((m.start(), i))
    return out


def string_literals(src: str) -> list[tuple[int, str, bool, int]]:
    """返回 (行号, 内容, 是否已被 tr!/trf!/i18n::tr(...) 包住, 起始字节偏移)。

    只取含 CJK 的字面量；注释与字符字面量已剥离。
    """
    literals: list[tuple[int, str, bool, int]] = []
    i, n, line, block = 0, len(src), 1, 0

    def wrapped(start: int) -> bool:
        """字面量开引号前的文本是否以某个取词调用（或本地化下拉包装）结尾。"""
        prefix = src[:start].rstrip()
        for tok in (
            "i18n::tr_in(",
            "i18n::tr(",
            "i18n::tr_dyn(",
            "tr!(",
            "trf!(",
            "tr_in(",
            "tr_dyn(",
            "LocalizedChoice::new(",
        ):
            if not prefix.endswith(tok):
                continue
            head = len(prefix) - len(tok)
            if head == 0 or not (prefix[head - 1].isalnum() or prefix[head - 1] == "_"):
                return True
        return False

    while i < n:
        c = src[i]
        if block == 0 and c == "/" and i + 1 < n and src[i + 1] == "/":
            j = src.find("\n", i)
            if j == -1:
                break
            i = j
            continue
        if c == "/" and i + 1 < n and src[i + 1] == "*":
            block += 1
            i += 2
            continue
        if block > 0:
            if c == "*" and i + 1 < n and src[i + 1] == "/":
                block -= 1
                i += 2
                continue
            if c == "\n":
                line += 1
            i += 1
            continue
        if c == "'":
            m = re.match(r"'(?:\\.|[^\\'])'", src[i:])
            if m:
                i += m.end()
                continue
            i += 1
            continue
        if c == "r" and i + 1 < n and src[i + 1] in '#"':
            m = re.match(r'r(#*)"', src[i:])
            if m:
                end = '"' + m.group(1)
                start = i + m.end()
                j = src.find(end, start)
                if j == -1:
                    break
                text = src[start:j]
                if CJK.search(text):
                    literals.append((line, text, wrapped(i), i))
                line += src.count("\n", i, j)
                i = j + len(end)
                continue
        if c == '"':
            j, buf = i + 1, []
            while j < n:
                ch = src[j]
                if ch == "\\":
                    buf.append(src[j : j + 2])
                    j += 2
                    continue
                if ch == '"':
                    break
                if ch == "\n":
                    line += 1
                buf.append(ch)
                j += 1
            text = "".join(buf)
            if CJK.search(text):
                literals.append((line, text, wrapped(i), i))
            i = j + 1
            continue
        if c == "\n":
            line += 1
        i += 1
    return literals


def classify(lines: list[str], ln: int, rel: str, lit_start: int = -1, src: str = "") -> str:
    # i18n 表自身（旧的结构体表 + 新的运行时目录）不算缺口。
    if "/crates/tools/extras/src/i18n" in "/" + rel or "/core/core/src/i18n" in "/" + rel:
        return "i18n-table"
    # 在字面量之前 500 字符内找「最近一个日志/断言宏调用」，且两者之间不得跨越语句边界（`;`）。
    # 这能同时覆盖单行与跨行的 `tracing::debug!(\n "…"\n)`。
    window_start = max(0, lit_start - 500) if lit_start >= 0 else 0
    window = src[window_start:lit_start] if lit_start >= 0 else "\n".join(lines[max(0, ln - 2) : ln])
    best = None
    for m in MACRO_OPEN.finditer(window):
        best = m
    if best is not None and ";" not in window[best.end() :]:
        tok = best.group(0)
        for key, name in (
            ("assert", "assert/panic"),
            ("panic", "assert/panic"),
            ("unreachable", "assert/panic"),
            ("todo", "assert/panic"),
            ("unimplemented", "assert/panic"),
            ("expect", "expect"),
            ("unwrap_or_else", "expect"),
            ("print", "print"),
            ("#[error", "thiserror"),
            ("impl_unit_enum_from_str", "enum-parse"),
        ):
            if key in tok:
                return name
        return "log"
    if 0 < ln <= len(lines) and "#[error" in lines[ln - 1]:
        return "thiserror"
    return "ui"


# --------------------------------------------------------------------------- 分桶


def bucket_of(rel: str) -> str:
    if rel.startswith("crates/editor/ui/src/view/"):
        return "ui_view"
    if rel.startswith("crates/editor/ui/src/"):
        return "ui_shell"
    if rel.startswith(
        (
            "crates/editor/ui-core/",
            "crates/editor/ui-editor/",
            "crates/editor/ui-settings/",
            "crates/editor/editor-state/",
        )
    ):
        return "ui_kit"
    if rel.startswith(("crates/editor/gfx/", "crates/editor/dialog/")):
        return "gfx_dialog"
    if rel.startswith("src/runner/menu/"):
        return "runner_menu"
    if rel.startswith("src/"):
        return "runner_host"
    if rel.startswith("crates/network/"):
        return "cloud_collab"
    if rel.startswith("crates/tools/"):
        return "export"
    if rel.startswith("crates/audio/"):
        return "audio"
    if rel.startswith("crates/core/"):
        return "project_core"
    return "misc"


def crate_of(rel: str) -> str:
    parts = rel.split("/")
    if parts[0] == "crates" and len(parts) > 2:
        return f"crates/{parts[1]}/{parts[2]}"
    if parts[0] == "src":
        return "src"
    return parts[0]


# --------------------------------------------------------------------------- 主流程


def scan() -> list[dict]:
    files = rs_files()
    tm = TestModules(files)
    records = []
    for full in files:
        rel = os.path.relpath(full, ROOT).replace("\\", "/")
        try:
            src = open(full, encoding="utf-8").read()
        except UnicodeDecodeError:
            continue
        lits = string_literals(src)
        if not lits:
            continue
        lines = src.split("\n")
        ranges = cfg_test_ranges(src)
        file_is_test = tm.is_test(full)
        for ln, text, is_wrapped, start_idx in lits:
            in_test = file_is_test
            if not in_test:
                for s, e in ranges:
                    if src[:s].count("\n") + 1 <= ln <= src[:e].count("\n") + 1:
                        in_test = True
                        break
            kind = "i18n-wrapped" if is_wrapped else classify(lines, ln, rel, start_idx, src)
            records.append(
                {
                    "file": rel,
                    "line": ln,
                    "text": text,
                    "ctx": lines[ln - 1].strip() if 0 < ln <= len(lines) else "",
                    "test": in_test,
                    "bucket": kind,
                }
            )
    return records


def write_buckets(records: list[dict], outdir: str) -> None:
    os.makedirs(outdir, exist_ok=True)
    groups: dict[str, list[dict]] = collections.defaultdict(list)
    for r in records:
        if not r["test"] and r["bucket"] == "ui":
            groups[bucket_of(r["file"])].append(r)
    for name, items in groups.items():
        by_file: dict[str, list[dict]] = collections.defaultdict(list)
        for r in items:
            by_file[r["file"]].append(r)
        lines = [f"# 工作桶 {name}：{len(items)} 条 / {len(by_file)} 个文件", ""]
        for f in sorted(by_file):
            lines.append(f"## {f}  ({len(by_file[f])} 条)")
            for r in sorted(by_file[f], key=lambda x: x["line"]):
                lines.append(f"- L{r['line']} | {r['text']}".replace("\n", "\\n"))
            lines.append("")
        open(os.path.join(outdir, f"{name}.txt"), "w", encoding="utf-8").write("\n".join(lines))


def report(records: list[dict]) -> str:
    prod = [r for r in records if not r["test"]]
    ui = [r for r in prod if r["bucket"] == "ui"]
    by_bucket = collections.Counter(r["bucket"] for r in prod)
    by_crate = collections.Counter(crate_of(r["file"]) for r in ui)
    by_file = collections.Counter(r["file"] for r in ui)
    out = []
    out.append("# i18n 覆盖缺口审计报告\n")
    out.append("> 由 `scripts/i18n_audit.py` 自动生成，勿手工编辑。\n")
    out.append("## 总览\n")
    out.append("| 口径 | 条数 |")
    out.append("|---|---|")
    out.append(f"| 含中文的字符串字面量（含测试） | {len(records)} |")
    out.append(f"| 其中测试 / 基准 / 示例 | {len(records) - len(prod)} |")
    out.append(f"| 生产代码 | {len(prod)} |")
    out.append(f"| **疑似用户可见文案（缺口主体）** | **{len(ui)}** |")
    out.append(f"| 涉及文件数（疑似文案） | {len(by_file)} |")
    out.append("")
    out.append("## 生产代码分类明细\n")
    out.append("| 分类 | 条数 | 是否算 i18n 缺口 |")
    out.append("|---|---:|---|")
    labels = {
        "ui": "✅ 是",
        "i18n-table": "— 表自身",
        "i18n-wrapped": "✅ 已覆盖（tr!/trf!）",
        "log": "❌ 否（开发者日志）",
        "assert/panic": "❌ 否",
        "expect": "❌ 否",
        "thiserror": "⚠️ 属性宏，需人工重构",
        "enum-parse": "⚠️ 枚举解析错误串",
        "print": "❌ 否",
    }
    for k, v in by_bucket.most_common():
        out.append(f"| `{k}` | {v} | {labels.get(k, '')} |")
    out.append("")
    out.append("## 疑似文案按 crate 分布\n")
    out.append("| crate | 条数 |")
    out.append("|---|---:|")
    for k, v in by_crate.most_common():
        out.append(f"| `{k}` | {v} |")
    out.append("")
    out.append("## 疑似文案单文件 Top 40\n")
    out.append("| 文件 | 条数 |")
    out.append("|---|---:|")
    for k, v in by_file.most_common(40):
        out.append(f"| `{k}` | {v} |")
    out.append("")
    out.append("## 全部疑似文案明细\n")
    cur = None
    for r in sorted(ui, key=lambda x: (x["file"], x["line"])):
        if r["file"] != cur:
            cur = r["file"]
            out.append(f"\n### `{cur}`\n")
        text = r["text"].replace("\n", "\\n")
        out.append(f"- L{r['line']} | {text}")
    out.append("")
    return "\n".join(out)


def main() -> int:
    ap = argparse.ArgumentParser(description="i18n 覆盖缺口审计")
    ap.add_argument("--json", help="输出逐条明细 JSON")
    ap.add_argument("--buckets", help="输出按目录域切分的工作清单目录")
    ap.add_argument("--report", help="输出 markdown 报告")
    args = ap.parse_args()

    records = scan()
    prod = [r for r in records if not r["test"]]
    ui = [r for r in prod if r["bucket"] == "ui"]

    print(f"含中文串总数: {len(records)}")
    print(f"  测试/基准/示例: {len(records) - len(prod)}")
    print(f"  生产代码:        {len(prod)}")
    print("  生产代码分类:")
    for k, v in collections.Counter(r["bucket"] for r in prod).most_common():
        print(f"    {k:14s} {v}")
    print(f"\n疑似用户可见文案（i18n 缺口主体）: {len(ui)} 条 / "
          f"{len(set(r['file'] for r in ui))} 文件")

    if args.json:
        json.dump(records, open(args.json, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
        print(f"明细 → {args.json}")
    if args.buckets:
        write_buckets(records, args.buckets)
        print(f"分桶清单 → {args.buckets}")
    if args.report:
        open(args.report, "w", encoding="utf-8").write(report(records))
        print(f"报告 → {args.report}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
