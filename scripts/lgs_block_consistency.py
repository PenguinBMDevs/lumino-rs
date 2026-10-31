#!/usr/bin/env python3
"""REF-LGS-008 块边界一致性护栏（分层口径）。

在同一素材/同一配置下，用不同 block_size 渲染并比较**公共前缀**：

- 同块长确定性：参考块长跑两遍，要求逐位相同；
- 跨块长：要求差异能量 ≤ 阈值（默认 -45 dBFS），且报告首个差异帧。
  逐位一致不再作为验收口径——声部死亡只能按块边界裁剪，尾段截断点必然
  随块长移动（见 docs/2026-10-31-REF-LGS-008-*）。

默认关闭限幅器（`LUMINO_NO_LIMITER=1`），用于验证「渲染层」本身；
加 `--limiter` 则验证默认路径（A2 之后 limiter 已按绝对帧对齐的 512 帧窗口
处理，不应再引入块长差异）。

用法：
    python scripts/lgs_block_consistency.py \
        --midi test-file/bench/sustained2048.mid \
        --sf2 test-file/bench/rich.sf2 \
        --frames 480000 --blocks 512,1024,2048,4096

    python scripts/lgs_block_consistency.py ... --limiter --blocks 512,2048
"""

from __future__ import annotations

import argparse
import math
import os
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_BIN = REPO_ROOT / "target" / "release" / "examples" / "hash_render.exe"

if os.name != "nt":
    DEFAULT_BIN = REPO_ROOT / "target" / "release" / "examples" / "hash_render"


def parse_blocks(text: str) -> list[int]:
    blocks = sorted({int(v) for v in text.replace(" ", "").split(",") if v})
    if not blocks:
        raise SystemExit("--blocks 不能为空")
    for b in blocks:
        if b < 16 or (b & (b - 1)) != 0:
            raise SystemExit(f"block {b} 必须是 >=16 的 2 的幂")
    return blocks


def render(
    exe: Path,
    midi: Path,
    sf2: Path,
    frames: int,
    block: int,
    out_raw: Path,
    limiter: bool,
    extra_args: list[str],
) -> str:
    """渲染一次并返回 hash_render 的 fnv1a64（同时落盘 raw f32）。"""
    cmd = [str(exe), str(midi), str(sf2), str(frames), str(block), "--raw", str(out_raw)]
    cmd.extend(extra_args)
    env = dict(os.environ)
    if limiter:
        env.pop("LUMINO_NO_LIMITER", None)
    else:
        env["LUMINO_NO_LIMITER"] = "1"
    proc = subprocess.run(cmd, capture_output=True, text=True, env=env, check=False)
    if proc.returncode != 0:
        raise SystemExit(f"渲染失败（block={block}）：\n{proc.stdout}\n{proc.stderr}")
    for line in proc.stdout.splitlines():
        if "fnv1a64=" in line:
            return line.rsplit("fnv1a64=", 1)[1].strip()
    raise SystemExit(f"未找到 fnv1a64（block={block}）：\n{proc.stdout}")


def load_raw(path: Path) -> bytes:
    return path.read_bytes()


def compare(a: bytes, b: bytes, guard_samples: int) -> tuple[int, int, float, float]:
    """比较公共前缀（去掉末尾 guard_samples 个 f32），返回统计量。"""
    n = min(len(a), len(b)) // 4
    guard = min(guard_samples // 1, max(n - 2, 0))
    n -= guard
    xa = struct.unpack_from(f"<{n}f", a)
    xb = struct.unpack_from(f"<{n}f", b)
    first = -1
    diff = 0
    se = 0.0
    mx = 0.0
    for i in range(n):
        d = xa[i] - xb[i]
        if d != 0.0:
            if first < 0:
                first = i
            diff += 1
            se += d * d
            ax = abs(d)
            if ax > mx:
                mx = ax
    rms = math.sqrt(se / n) if n else 0.0
    db = 20.0 * math.log10(rms) if rms > 0.0 else -999.0
    return first, diff, mx, db


def main() -> int:
    ap = argparse.ArgumentParser(description="REF-LGS-008 块边界一致性护栏")
    ap.add_argument("--exe", type=Path, default=DEFAULT_BIN)
    ap.add_argument("--midi", type=Path, required=True)
    ap.add_argument("--sf2", type=Path, required=True)
    ap.add_argument("--frames", type=int, default=480_000)
    ap.add_argument("--blocks", type=str, default="512,1024,2048,4096")
    ap.add_argument("--reference", type=int, default=0, help="参考块长（默认取最小块）")
    ap.add_argument("--limiter", action="store_true", help="开启限幅器（默认关闭）")
    ap.add_argument("--rms-dbfs", type=float, default=-45.0, help="差异 RMS 上限（dBFS）")
    ap.add_argument("--max-abs", type=float, default=None, help="可选：单样本差上限")
    ap.add_argument("--extra-args", type=str, default="", help="透传 hash_render 参数")
    ap.add_argument("--json", type=Path, default=None)
    args = ap.parse_args()

    if not args.exe.exists():
        raise SystemExit(f"找不到 {args.exe}（先 cargo build --release --example hash_render）")

    blocks = parse_blocks(args.blocks)
    ref = args.reference or blocks[0]
    if ref not in blocks:
        blocks.append(ref)
        blocks.sort()
    extra = args.extra_args.split() if args.extra_args else []
    # 末尾一个最大块长的两倍作为安全区：离线渲染按整块停止，最后一个块是否
    # 参与渲染取决于块长，属于「长度」而不是「内容」差异。
    guard_samples = max(blocks) * 4

    rows = []
    with tempfile.TemporaryDirectory(prefix="lgs_block_") as td:
        tmp = Path(td)
        ref_raw = tmp / f"ref_{ref}.f32"
        ref_hash = render(args.exe, args.midi, args.sf2, args.frames, ref, ref_raw, args.limiter, extra)
        # 同块长确定性：再跑一遍，必须逐位相同。
        ref_raw2 = tmp / f"ref_{ref}_repeat.f32"
        ref_hash2 = render(args.exe, args.midi, args.sf2, args.frames, ref, ref_raw2, args.limiter, extra)
        deterministic = ref_hash == ref_hash2 and load_raw(ref_raw) == load_raw(ref_raw2)
        ref_bytes = load_raw(ref_raw)

        for block in blocks:
            raw = tmp / f"b{block}.f32"
            h = render(args.exe, args.midi, args.sf2, args.frames, block, raw, args.limiter, extra)
            if block == ref:
                first, diff, mx, db = 0, 0, 0.0, -999.0
            else:
                first, diff, mx, db = compare(ref_bytes, load_raw(raw), guard_samples)
            rows.append(
                {
                    "block": block,
                    "hash": h,
                    "first_frame": (first // 2) if first >= 0 else None,
                    "diff_samples": diff,
                    "max_abs": mx,
                    "rms_dbfs": db,
                }
            )

    print(f"=== REF-LGS-008 block consistency (ref={ref}, limiter={'on' if args.limiter else 'off'}) ===")
    print(f"midi={args.midi.name} frames={args.frames} rms_limit={args.rms_dbfs:.1f} dBFS")
    print(f"{'block':>6} {'first_diff_frame':>17} {'diff_samples':>13} {'max|d|':>10} {'rms(dBFS)':>11}  verdict")
    failed = False
    for r in rows:
        if r["block"] == ref:
            verdict = "reference"
        else:
            verdict = "ok"
            if r["rms_dbfs"] > args.rms_dbfs:
                verdict = "FAIL(rms)"
                failed = True
            if args.max_abs is not None and r["max_abs"] > args.max_abs:
                verdict = "FAIL(max)"
                failed = True
        if not deterministic:
            verdict = "FAIL(nondeterministic)"
            failed = True
        first = "-" if r["first_frame"] is None else str(r["first_frame"])
        print(
            f"{r['block']:>6} {first:>17} {r['diff_samples']:>13} "
            f"{r['max_abs']:>10.4g} {r['rms_dbfs']:>11.1f}  {verdict}"
        )
    print(f"same-block determinism: {'OK' if deterministic else 'FAILED'}")

    if args.json:
        import json

        args.json.write_text(
            json.dumps(
                {
                    "midi": str(args.midi),
                    "sf2": str(args.sf2),
                    "frames": args.frames,
                    "limiter": args.limiter,
                    "reference": ref,
                    "rms_limit_dbfs": args.rms_dbfs,
                    "deterministic": deterministic,
                    "rows": rows,
                    "ok": not failed,
                },
                ensure_ascii=False,
                indent=2,
            )
            + "\n",
            encoding="utf-8",
        )

    print("VERDICT:", "PASS" if not failed else "FAIL")
    return 0 if not failed else 1


if __name__ == "__main__":
    sys.exit(main())
