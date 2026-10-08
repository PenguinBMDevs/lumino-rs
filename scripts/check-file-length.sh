#!/usr/bin/env bash
# 文件行数守卫（DEBT-09 #126）：仓库硬规「单文件 < 400 行」。
#
# 本批次已拆分目标文件（depth_tests / perf_tests / brush tests / processor /
# timeline_canvas / about_egg）；存量 >400 行文件显式列入白名单
# （scripts/file-length-whitelist.txt，含理由），未列入白名单的超限文件会使
# 本守卫失败——防止新债产生。新增文件请先拆分，确需保留则登记理由。
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WL="$ROOT/scripts/file-length-whitelist.txt"
LIMIT=400

if [ ! -f "$WL" ]; then
  echo "缺少白名单文件：$WL" >&2
  exit 1
fi

fail=0
count=0
while IFS= read -r -d '' f; do
  lines=$(wc -l < "$f")
  if [ "$lines" -gt "$LIMIT" ]; then
    rel="${f#"$ROOT"/}"
    count=$((count + 1))
    if ! grep -qxF "$rel" "$WL"; then
      echo "超限：$rel（$lines 行 > $LIMIT）未列入白名单"
      fail=1
    fi
  fi
done < <(find "$ROOT/crates" "$ROOT/src" "$ROOT/tests" -name '*.rs' \
  -not -path '*/target/*' -print0 2>/dev/null)

if [ "$fail" -ne 0 ]; then
  echo "文件行数守卫失败：请拆分超限文件，或在 scripts/file-length-whitelist.txt 登记理由。" >&2
  exit 1
fi
echo "文件行数守卫通过：超限文件全部在白名单内（共 $count 个存量）。"
