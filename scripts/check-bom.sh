#!/usr/bin/env bash
# UTF-8 BOM 守卫：全仓 *.rs 文件头不得出现 BOM（EF BB BF）。
#
# 背景：670feddb / aeba65e0 期间（2026-10-01）编辑器保存行为把 BOM 写进了若干
# 源文件，污染 diff、代码搜索与 gfx shader 契约守卫测试。本脚本作为与
# fmt/clippy 同级的 lint 门禁，命中任意 .rs 即失败并打印命中列表。
#
# 用法： bash scripts/check-bom.sh     （CI / 任意平台，不依赖可执行位）
#        ./scripts/check-bom.sh        （本地 bash，需可执行位）
#
# 退出码：0 = 无 BOM；1 = 命中 BOM；2 = 脚本自身异常（不在 git 树 / 扫描范围为空 / 探测器失效）
#
# 实现约束：只使用 POSIX 语法（无 mapfile / 无数组 / 无 $'...'）。
# 曾被 dash 执行时 mapfile 与 ${#arr[@]} 直接语法报错，门禁退化为 exit 2 空转。
set -u

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null)"
if [ -z "${REPO_ROOT}" ]; then
    echo "::error::不在 git 工作树内，无法确定扫描范围" >&2
    exit 2
fi
cd "${REPO_ROOT}" || exit 2

# 只锚定文件头部 3 字节（EF BB BF）。
# ⚠️ 两处坑必须同时规避：
#   1) 用 printf 的八进制转义生成字节，而非 $'...'（后者非 POSIX，dash 下失效）；
#   2) 必须带 LC_ALL=C —— UTF-8 locale（CI 默认 C.UTF-8）下 \xEF 会被当成码点
#      U+00EF（编码 C3 AF）而非字节 0xEF。实测未加 LC_ALL=C 时仓库内 4 个
#      BOM 文件全部漏检，守卫静默永不报警。
BOM_PATTERN="$(printf '^\357\273\277')"

# ── 探测器自检（canary）──────────────────────────────────────────────────
# 正例必须命中、反例必须不命中，否则检测逻辑已退化，直接红——杜绝"门禁空转"。
CANARY_DIR="$(mktemp -d)" || exit 2
trap 'rm -rf "${CANARY_DIR}"' EXIT
printf '\357\273\277fn main() {}\n' > "${CANARY_DIR}/with_bom.rs"
printf 'fn main() {}\n' > "${CANARY_DIR}/without_bom.rs"

if ! LC_ALL=C grep -q -- "${BOM_PATTERN}" "${CANARY_DIR}/with_bom.rs"; then
    echo "::error::BOM 探测器自检失败：未能命中已知 BOM 正例，检测逻辑已失效" >&2
    exit 2
fi
if LC_ALL=C grep -q -- "${BOM_PATTERN}" "${CANARY_DIR}/without_bom.rs"; then
    echo "::error::BOM 探测器自检失败：对无 BOM 反例误报" >&2
    exit 2
fi

# ── 扫描范围：全仓 tracked + untracked（不含 .gitignore 忽略项）的 *.rs ──
# 用 git ls-files 而非 grep -r：自动排除 target/ 等构建产物。
TOTAL="$(git ls-files --cached --others --exclude-standard -- '*.rs' | wc -l | tr -d ' ')"
if [ "${TOTAL}" -eq 0 ]; then
    echo "::error::未匹配到任何 .rs 文件，扫描范围为空（构建上下文异常）" >&2
    exit 2
fi

HITS="$(git ls-files -z --cached --others --exclude-standard -- '*.rs' \
    | LC_ALL=C xargs -0 grep -l -- "${BOM_PATTERN}" 2>/dev/null || true)"

if [ -n "${HITS}" ]; then
    COUNT="$(printf '%s\n' "${HITS}" | wc -l | tr -d ' ')"
    echo "::error::检测到 UTF-8 BOM（EF BB BF）：${COUNT} 个 .rs 文件命中"
    printf '%s\n' "${HITS}" | sed 's/^/  BOM: /'
    echo
    echo "修复：仅去掉文件头 3 字节，其余内容逐字节保持不变（禁止顺手格式化/重排）。"
    echo "排查源头：能把 BOM 写回源文件的编辑器/工具保存设置。"
    exit 1
fi

echo "BOM 守卫通过：已扫描 ${TOTAL} 个 .rs 文件，无 BOM 命中。"
