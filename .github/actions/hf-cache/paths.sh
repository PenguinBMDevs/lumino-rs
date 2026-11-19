# HF CI Cache —— PATHS 解析与校验。
#
# 约定：PATHS 每行一个**仓库相对路径**（如 target、target/release）。
# 不接受 ~、绝对路径、盘符路径、反斜杠路径或含 .. 的路径；. 与 ./ 前缀会归一化。
# 打包端与解压端都走这里，保证 tar 成员名与还原位置永远一致。
#
# 用法：source "$GITHUB_ACTION_PATH/paths.sh"
#       hf_cache_roots            # 读取 env PATHS，逐行输出校验后的路径
#       hf_cache_member_exists P MEMBER_FILE

# 去掉 Windows CRLF 残留与首尾空白（YAML 输入在 Windows 上可能带 \r）。
hf_cache_trim() {
  local s="$1"
  s="${s%$'\r'}"
  s="${s#"${s%%[![:space:]]*}"}"
  s="${s%"${s##*[![:space:]]}"}"
  printf '%s' "$s"
}

# 读取 PATHS，校验并输出仓库相对路径；任一非法路径返回非零。
hf_cache_roots() {
  local raw p
  while IFS= read -r raw; do
    p="$(hf_cache_trim "$raw")"
    if [ -z "$p" ]; then continue; fi

    # 归一化 ./ 前缀与尾部 /，避免同一目录出现两种成员名。
    while [ "${p#./}" != "$p" ]; do p="${p#./}"; done
    while [ "${p%/}" != "$p" ]; do p="${p%/}"; done

    if [ -z "$p" ] || [ "$p" = "." ]; then
      echo "::error::缓存路径不能是仓库根目录" >&2
      return 1
    fi
    case "$p" in
      /*|~*|[A-Za-z]:*|*\\*)
        echo "::error::缓存路径必须是仓库内相对路径（不接受 ~、绝对路径或反斜杠）: $p" >&2
        return 1
        ;;
    esac
    case "/$p/" in
      */../*)
        echo "::error::缓存路径不能包含 ..: $p" >&2
        return 1
        ;;
    esac

    printf '%s\n' "$p"
  done <<< "${PATHS:-}"
}

# 判断 tar 成员列表里是否存在 root 本身或其子项。
# 用 index() 做字面前缀匹配，避免路径里的正则元字符造成误判。
hf_cache_member_exists() {
  local root="$1" members="$2"
  awk -v r="$root" '
    index($0, r) == 1 {
      rest = substr($0, length(r) + 1)
      if (rest == "" || substr(rest, 1, 1) == "/") { found = 1; exit }
    }
    END { exit(found ? 0 : 1) }
  ' "$members"
}

# 选一个可用的 Python：Windows 优先 python（避免 Store 的 python3 别名），
# 其余平台优先 python3（macOS 默认没有 python）。
hf_cache_python() {
  local py
  if [ "${RUNNER_OS:-}" = "Windows" ]; then
    py="$(command -v python || command -v python3 || true)"
  else
    py="$(command -v python3 || command -v python || true)"
  fi
  printf '%s' "$py"
}

# 确保 huggingface_hub 可用。安装和使用必须在同一个 shell、同一个解释器里，
# 避免跨步骤 PATH/解释器漂移（canary 上传步骤就是这么做的）。
# PEP 668（externally-managed-environment）环境下退到 --break-system-packages；
# runner 是一次性环境，允许这个兜底。
hf_cache_ensure_hf_hub() {
  local py="$1"
  if "$py" -c 'import huggingface_hub' >/dev/null 2>&1; then
    return 0
  fi
  # 装完必须用「同一个解释器」再 import 一次，防止 pip 装到别的 site 却假装成功。
  if "$py" -m pip install --quiet --upgrade huggingface_hub >/dev/null 2>&1 \
    && "$py" -c 'import huggingface_hub' >/dev/null 2>&1; then
    return 0
  fi
  # 少数 runner 的 python 没带 pip：先 ensurepip 再试一次。
  "$py" -m ensurepip --upgrade >/dev/null 2>&1 || true
  if "$py" -m pip install --quiet --upgrade huggingface_hub >/dev/null 2>&1 \
    && "$py" -c 'import huggingface_hub' >/dev/null 2>&1; then
    return 0
  fi
  # PEP 668（externally-managed-environment）兜底；runner 是一次性环境。
  "$py" -m pip install --quiet --upgrade --break-system-packages huggingface_hub \
    && "$py" -c 'import huggingface_hub' >/dev/null 2>&1
}

# RUNNER_TEMP 在 Windows 上是盘符路径（D:\a\_temp），Git Bash 下统一成 POSIX。
hf_cache_temp_dir() {
  if command -v cygpath >/dev/null 2>&1; then
    cygpath -u "$RUNNER_TEMP"
  else
    printf '%s' "$RUNNER_TEMP"
  fi
}
