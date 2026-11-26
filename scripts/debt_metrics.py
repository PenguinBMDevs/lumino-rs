#!/usr/bin/env python3
"""DEBT-10 债务度量：文件 / 函数尺寸、god object、功能分散度。

口径与 `docs/2026-11-26-DEBT-10-结构减法与尺寸治理.md` 一致：

- 行数按 `\\n` 计数 + 1，排除 `docs/`、`target/`、`.git/`、`.ai-context/`；
- 函数按 `fn ` 起始、花括号配平结束，剔除 `#[cfg(test)]` 之外的误报；
- 输出 Markdown 或 JSON，供周报直接引用。

用法：
    python scripts/debt_metrics.py                 # Markdown 摘要
    python scripts/debt_metrics.py --json          # JSON（供 diff）
    python scripts/debt_metrics.py --baseline FILE # 与上次 JSON 对比
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

SKIP_DIRS = {"docs", "target", ".git", ".ai-context", "node_modules"}
FILE_LIMIT = 600
TEST_FILE_LIMIT = 800
FUNCTION_LIMIT = 100

GOD_OBJECT_RE = re.compile(r"^impl\s+([A-Z][A-Za-z0-9_]*)\s*\{", re.MULTILINE)
STRUCT_RE = re.compile(r"^pub(?:\([^)]*\))?\s+struct\s+([A-Z][A-Za-z0-9_]*)\s*\{", re.MULTILINE)
FN_RE = re.compile(
    r"^(?P<indent>\s*)(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(?P<name>[A-Za-z0-9_]+)"
)


def rust_files(root: Path):
    for path in root.rglob("*.rs"):
        if any(part in SKIP_DIRS for part in path.parts):
            continue
        yield path


def count_lines(path: Path) -> int:
    data = path.read_bytes()
    return data.count(b"\n") + 1


def is_test_file(path: Path, text: str) -> bool:
    """仅按路径判定测试文件。

    不能用「文件里出现 `#[cfg(test)]`」——大量生产文件底部挂着
    `#[cfg(test)] mod tests;`，会把它们错判成测试文件。
    """
    del text  # 保留参数，调用方语义不变
    name = path.name
    posix = path.as_posix()
    return name.endswith("tests.rs") or "/tests/" in posix


def scan_functions(text: str):
    """返回 (函数名, 起始行, 行数)。花括号配平，跳过字符串。"""
    lines = text.split("\n")
    results = []
    i = 0
    while i < len(lines):
        m = FN_RE.match(lines[i])
        if not m:
            i += 1
            continue
        start = i
        depth = 0
        started = False
        in_str = False
        j = i
        while j < len(lines):
            k = 0
            line = lines[j]
            while k < len(line):
                ch = line[k]
                if in_str:
                    if ch == "\\":
                        k += 2
                        continue
                    if ch == '"':
                        in_str = False
                else:
                    if ch == '"':
                        in_str = True
                    elif ch == "{":
                        depth += 1
                        started = True
                    elif ch == "}":
                        depth -= 1
                k += 1
            if started and depth <= 0:
                break
            j += 1
        if started and j < len(lines):
            results.append((m.group("name"), start + 1, j - start + 1))
            i = j + 1
        else:
            i += 1
    return results


def impl_method_counts(text: str):
    """统计 `impl Type {` 块内的方法数：按花括号配平切块。"""
    counts = {}
    lines = text.split("\n")
    i = 0
    while i < len(lines):
        m = GOD_OBJECT_RE.match(lines[i])
        if not m:
            i += 1
            continue
        name = m.group(1)
        depth = lines[i].count("{") - lines[i].count("}")
        j = i + 1
        methods = 0
        while j < len(lines) and depth > 0:
            stripped = lines[j].strip()
            if stripped.startswith("fn ") or stripped.startswith("pub fn ") or stripped.startswith(
                "pub(crate) fn "
            ):
                methods += 1
            depth += lines[j].count("{") - lines[j].count("}")
            j += 1
        counts[name] = counts.get(name, 0) + methods
        i = j
    return counts


def struct_field_counts(text: str):
    counts = {}
    for m in STRUCT_RE.finditer(text):
        name = m.group(1)
        tail = text[m.end() :]
        depth = 1
        fields = 0
        for line in tail.split("\n"):
            stripped = line.strip()
            if depth == 1 and stripped and not stripped.startswith(("//", "#", "}")):
                if re.match(r"^(pub(?:\([^)]*\))?\s+)?[A-Za-z_][A-Za-z0-9_]*\s*:", stripped):
                    fields += 1
            depth += line.count("{") - line.count("}")
            if depth <= 0:
                break
        counts[name] = max(counts.get(name, 0), fields)
    return counts


def collect(root: Path, feature_keywords):
    files = []
    funcs = []
    impl_methods = {}
    struct_fields = {}
    feature_files = {key: 0 for key in feature_keywords}
    feature_lines = {key: 0 for key in feature_keywords}

    for path in rust_files(root):
        rel = path.relative_to(root).as_posix()
        text = path.read_text(encoding="utf-8", errors="replace")
        lines = count_lines(path)
        test = is_test_file(path, text)
        files.append({"path": rel, "lines": lines, "test": test})

        funcs.extend(
            {"path": rel, "name": name, "line": line, "lines": size}
            for name, line, size in scan_functions(text)
        )

        for name, n in impl_method_counts(text).items():
            impl_methods[name] = impl_methods.get(name, 0) + n
        for name, n in struct_field_counts(text).items():
            struct_fields[name] = max(struct_fields.get(name, 0), n)

        for key in feature_keywords:
            if key in text:
                feature_files[key] += 1
                feature_lines[key] += lines

    return {
        "files": files,
        "functions": funcs,
        "impl_methods": impl_methods,
        "struct_fields": struct_fields,
        "feature_files": feature_files,
        "feature_lines": feature_lines,
    }


def summary(data):
    files = data["files"]
    funcs = data["functions"]
    prod = [f for f in files if not f["test"]]
    tests = [f for f in files if f["test"]]
    return {
        "files_total": len(files),
        "lines_total": sum(f["lines"] for f in files),
        "files_over_600": sorted(
            [f for f in files if f["lines"] > FILE_LIMIT], key=lambda f: -f["lines"]
        ),
        "test_files_over_800": sorted(
            [f for f in tests if f["lines"] > TEST_FILE_LIMIT], key=lambda f: -f["lines"]
        ),
        "production_files_over_600": len([f for f in prod if f["lines"] > FILE_LIMIT]),
        "functions_over_100": sorted(
            [f for f in funcs if f["lines"] > FUNCTION_LIMIT], key=lambda f: -f["lines"]
        ),
        "functions_over_200": sorted(
            [f for f in funcs if f["lines"] > 200], key=lambda f: -f["lines"]
        ),
        "god_objects": sorted(data["impl_methods"].items(), key=lambda kv: -kv[1])[:20],
        "wide_structs": sorted(data["struct_fields"].items(), key=lambda kv: -kv[1])[:20],
    }


def render_markdown(s):
    out = []
    out.append("# DEBT-10 债务度量\n")
    out.append(f"- `.rs` 文件：{s['files_total']}，总行数：{s['lines_total']:,}")
    out.append(f"- 生产 `>{FILE_LIMIT}` 行文件：{s['production_files_over_600']}")
    out.append(f"- `>200` 行函数：{len(s['functions_over_200'])}；`>100` 行函数：{len(s['functions_over_100'])}")
    out.append("\n## 生产 `>600` 行文件\n")
    for f in s["files_over_600"]:
        if not f["test"]:
            out.append(f"- {f['lines']:>5}  `{f['path']}`")
    out.append("\n## 测试 `>800` 行文件\n")
    for f in s["test_files_over_800"]:
        out.append(f"- {f['lines']:>5}  `{f['path']}`")
    out.append("\n## `>200` 行函数\n")
    for f in s["functions_over_200"]:
        out.append(f"- {f['lines']:>5}  `{f['name']}` @ `{f['path']}:{f['line']}`")
    out.append("\n## God object（impl 方法数 Top 20）\n")
    for name, n in s["god_objects"]:
        out.append(f"- `{name}`：{n} 个方法")
    out.append("\n## 宽 struct（字段数 Top 20）\n")
    for name, n in s["wide_structs"]:
        out.append(f"- `{name}`：{n} 个字段")
    return "\n".join(out)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=".")
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--baseline", help="上次 JSON 输出，用于差值")
    args = parser.parse_args()

    root = Path(args.root).resolve()
    data = collect(
        root,
        [
            "video_export",
            "chrome",
            "settings",
            "shape_ops",
            "toolbar",
            "collaboration",
            "right_sidebar",
            "shape_drawer",
        ],
    )
    s = summary(data)

    if args.json:
        print(json.dumps(s, ensure_ascii=False, indent=2))
        return 0

    text = render_markdown(s)
    if args.baseline:
        prev = json.loads(Path(args.baseline).read_text(encoding="utf-8"))
        text += "\n\n## 与基线差值\n"
        text += f"- 文件数：{s['files_total'] - prev['files_total']:+d}\n"
        text += f"- 总行数：{s['lines_total'] - prev['lines_total']:+d}\n"
        text += (
            f"- 生产 `>600` 文件："
            f"{s['production_files_over_600'] - prev['production_files_over_600']:+d}\n"
        )
        text += (
            f"- `>200` 行函数："
            f"{len(s['functions_over_200']) - len(prev['functions_over_200']):+d}\n"
        )
    print(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
