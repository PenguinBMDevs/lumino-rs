#!/usr/bin/env python3
"""REF-LGS-013 A/B 基准工具：交替顺序 + 中位数 + GPU 时钟/温度 + JSON/CSV。

背景：003 的教训是**单次对比会被环境漂移欺骗**（同一份代码同一天测得
8.33 → 9.17 → 9.60 s）。本工具固定以下纪律：

1. baseline / after 都以**独立可执行文件**参与，不重新编译；
2. 每对交替顺序（奇数对 baseline 先跑，偶数对 after 先跑），抵消热漂移；
3. 默认 6 对，取中位数；同时输出每对 delta 与符号一致性；
4. 每次运行前记录 GPU 时钟/温度/功耗（nvidia-smi，缺失时降级）；
5. 输出机器可读 JSON（汇总）与 CSV（逐次运行），便于归档/CI；
6. 校验两侧样本哈希（默认要求逐位一致；007 量化等场景用 --allow-hash-diff）；
7. 负向门控用例：窄通道配置必须告警且不 panic，宽/自动通道哈希一致。

用法::

    python scripts/lgs_ab.py \
        --baseline test-file/bench/hash_render_baseline.exe \
        --after    test-file/bench/hash_render_after.exe \
        --label 003-negative --pairs 6 --gate-pct 2 \
        --json test-file/bench/ab_003.json \
        --csv  test-file/bench/ab_003.csv

    # 只跑多端口用例
    python scripts/lgs_ab.py --baseline ... --after ... --only multiport_16p@512

退出码：0 = 通过（或 --fail-on none）；1 = 命中失败条件。
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import os
import platform
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_CASES = REPO_ROOT / "scripts" / "lgs_bench_cases.json"
DEFAULT_MANIFEST = REPO_ROOT / "test-file" / "bench" / "multiport_manifest.json"

WALL_RE = re.compile(r"wall=([0-9.]+)s audio=([0-9.]+)s realtime=([0-9.]+)x")
HASH_RE = re.compile(r"samples=(\d+) fnv1a64=([0-9a-fA-F]{16})")
PEAK_RE = re.compile(r"peak=([0-9.]+) rms=([0-9.]+)")
PORT_RE = re.compile(r"midi_max_port=(\d+) midi_channels=(\d+) auto_channels=(\w+)")
VOICE_RE = re.compile(
    r"voices_peak=(\d+) voice_frames=(\d+) voice_frames_per_s=([0-9.]+)"
)
SAMPLE_RE = re.compile(r"sample_gpu_bytes=(\d+) sample_cpu_bytes=(\d+)")
PHASE_RE = re.compile(
    r"apply=(\d+)us collect=(\d+)us sync=(\d+)us upload=(\d+)us "
    r"samples=(\d+)us mix=(\d+)us dispatch=(\d+)us readback=(\d+)us total=(\d+)us"
)
PHASES = ("apply", "collect", "sync", "upload", "samples", "mix", "dispatch", "readback", "total")


# ---------------------------------------------------------------------------
# 基础工具
# ---------------------------------------------------------------------------


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest().upper()


def median(values: list[float]) -> float:
    return statistics.median(values) if values else float("nan")


def binary_supports(exe: Path, flag: str) -> bool:
    """通过可执行文件里的参数串判断能力（旧二进制无 --json/--auto-channels）。"""
    try:
        return flag.encode() in exe.read_bytes()
    except OSError:
        return False


def query_gpu() -> dict:
    """读取 GPU 时钟/温度/功耗；无 nvidia-smi 时返回空 dict。"""
    exe = shutil.which("nvidia-smi")
    if not exe:
        return {}
    fields = "name,driver_version,clocks.sm,clocks.mem,temperature.gpu,power.draw,utilization.gpu"
    try:
        out = subprocess.run(
            [exe, f"--query-gpu={fields}", "--format=csv,noheader,nounits"],
            capture_output=True,
            text=True,
            timeout=10,
        )
    except (OSError, subprocess.TimeoutExpired):
        return {}
    if out.returncode != 0 or not out.stdout.strip():
        return {}
    parts = [p.strip() for p in out.stdout.strip().splitlines()[0].split(",")]
    keys = ("name", "driver", "sm_mhz", "mem_mhz", "temp_c", "power_w", "util_pct")
    gpu: dict = {}
    for key, raw in zip(keys, parts):
        try:
            gpu[key] = float(raw)
        except ValueError:
            gpu[key] = raw
    return gpu


def gpu_stamp(gpu: dict) -> dict:
    """一次运行前的轻量 GPU 快照（只保留数值字段）。"""
    return {
        k: gpu[k]
        for k in ("sm_mhz", "mem_mhz", "temp_c", "power_w", "util_pct")
        if k in gpu
    }


# ---------------------------------------------------------------------------
# 运行与解析
# ---------------------------------------------------------------------------


@dataclass
class Run:
    side: str
    pair: int
    first: str
    wall_s: float
    audio_s: float
    realtime: float
    samples: int
    fnv1a64: str
    peak: float | None = None
    rms: float | None = None
    midi_channels: int | None = None
    auto_channels: bool | None = None
    voices_peak: int | None = None
    voice_frames: int | None = None
    voice_frames_per_s: float | None = None
    sample_gpu_bytes: int | None = None
    sample_cpu_bytes: int | None = None
    phases: dict = field(default_factory=dict)
    gpu: dict = field(default_factory=dict)
    returncode: int = 0
    stderr_tail: str = ""


def parse_phases(stderr: str) -> dict:
    """从 LUMINO_PROFILE 的 `[profile] block` 行提取分相中位数（µs）。"""
    samples: dict[str, list[float]] = {k: [] for k in PHASES}
    for line in stderr.splitlines():
        if not line.startswith("[profile] block"):
            continue
        m = PHASE_RE.search(line)
        if not m:
            continue
        for name, value in zip(PHASES, m.groups()):
            samples[name].append(float(value))
    return {k: median(v) for k, v in samples.items() if v}


def parse_stdout(stdout: str) -> dict | None:
    m = WALL_RE.search(stdout)
    h = HASH_RE.search(stdout)
    if not m or not h:
        return None
    data = {
        "wall_s": float(m.group(1)),
        "audio_s": float(m.group(2)),
        "realtime": float(m.group(3)),
        "samples": int(h.group(1)),
        "fnv1a64": h.group(2).lower(),
    }
    p = PEAK_RE.search(stdout)
    if p:
        data["peak"] = float(p.group(1))
        data["rms"] = float(p.group(2))
    port = PORT_RE.search(stdout)
    if port:
        data["midi_max_port"] = int(port.group(1))
        data["midi_channels"] = int(port.group(2))
        data["auto_channels"] = port.group(3) == "true"
    voice = VOICE_RE.search(stdout)
    if voice:
        data["voices_peak"] = int(voice.group(1))
        data["voice_frames"] = int(voice.group(2))
        data["voice_frames_per_s"] = float(voice.group(3))
    sample = SAMPLE_RE.search(stdout)
    if sample:
        data["sample_gpu_bytes"] = int(sample.group(1))
        data["sample_cpu_bytes"] = int(sample.group(2))
    return data


def run_once(
    exe: Path,
    case: dict,
    *,
    side: str,
    pair: int,
    first: str,
    use_json: bool,
    profile: bool,
    gpu: dict,
    extra_args: list[str] | None = None,
    timeout: int = 1800,
) -> Run:
    midi = REPO_ROOT / case["midi"]
    sf2 = REPO_ROOT / case["sf2"]
    tmp_json = Path(tempfile.gettempdir()) / f"lgs_ab_{os.getpid()}_{side}_{pair}.json"
    tmp_json.unlink(missing_ok=True)

    cmd = [str(exe), str(midi), str(sf2), str(case["frames"]), str(case["block"])]
    if case.get("auto_channels"):
        cmd.append("--auto-channels")
    elif "channels" in case:
        cmd += ["--channels", str(case["channels"])]
    if use_json:
        cmd += ["--json", str(tmp_json)]
    if case.get("prewarm"):
        cmd.append("--prewarm")
    if extra_args:
        cmd += extra_args

    env = dict(os.environ)
    if profile:
        env["LUMINO_PROFILE"] = "1"
    if case.get("no_limiter"):
        env["LUMINO_NO_LIMITER"] = "1"

    started = time.perf_counter()
    proc = subprocess.run(
        cmd,
        cwd=str(REPO_ROOT),
        env=env,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=timeout,
    )
    elapsed = time.perf_counter() - started

    data: dict | None = None
    if use_json and tmp_json.exists():
        try:
            data = json.loads(tmp_json.read_text(encoding="utf-8"))
        finally:
            tmp_json.unlink(missing_ok=True)
    if data is None:
        data = parse_stdout(proc.stdout)
    if data is None:
        raise RuntimeError(
            f"{exe.name} 未产出可解析指标（rc={proc.returncode}）\n"
            f"stdout:\n{proc.stdout[-2000:]}\nstderr:\n{proc.stderr[-2000:]}"
        )

    return Run(
        side=side,
        pair=pair,
        first=first,
        wall_s=float(data["wall_s"]),
        audio_s=float(data["audio_s"]),
        realtime=float(data["realtime"]),
        samples=int(data["samples"]),
        fnv1a64=str(data["fnv1a64"]).lower(),
        peak=data.get("peak"),
        rms=data.get("rms"),
        midi_channels=data.get("midi_channels"),
        auto_channels=data.get("auto_channels"),
        voices_peak=data.get("voices_peak"),
        voice_frames=data.get("voice_frames"),
        voice_frames_per_s=data.get("voice_frames_per_s"),
        sample_gpu_bytes=data.get("sample_gpu_bytes"),
        sample_cpu_bytes=data.get("sample_cpu_bytes"),
        phases=parse_phases(proc.stderr),
        gpu=gpu_stamp(gpu),
        returncode=proc.returncode,
        stderr_tail=proc.stderr[-4000:],
    )


# ---------------------------------------------------------------------------
# 用例与清单校验
# ---------------------------------------------------------------------------


def load_cases(path: Path) -> dict:
    data = json.loads(path.read_text(encoding="utf-8"))
    if not data.get("cases"):
        raise SystemExit(f"{path} 没有 cases")
    return data


def verify_fixture(path: Path, manifest: dict) -> str | None:
    """按 manifest 校验尺寸/哈希；返回错误描述或 None。"""
    entry = next((f for f in manifest.get("files", []) if f["name"] == path.name), None)
    if entry is None:
        return None
    actual_size = path.stat().st_size
    if actual_size != entry["size"]:
        return f"{path.name} 尺寸不符：{actual_size} != {entry['size']}"
    actual = sha256_file(path)
    if actual != entry["sha256"]:
        return f"{path.name} SHA256 不符：{actual} != {entry['sha256']}"
    return None


# ---------------------------------------------------------------------------
# A/B 主流程
# ---------------------------------------------------------------------------


def run_case(
    case: dict,
    args: argparse.Namespace,
    gpu: dict,
    use_json: bool,
) -> dict:
    base_exe = Path(args.baseline).resolve()
    after_exe = Path(args.after).resolve()

    for _ in range(args.warmup):
        for exe, side in ((base_exe, "baseline"), (after_exe, "after")):
            run_once(
                exe,
                case,
                side=side,
                pair=0,
                first="warmup",
                use_json=use_json,
                profile=args.profile,
                gpu=gpu,
                extra_args=case.get("extra_args"),
                timeout=args.timeout,
            )

    pairs: list[dict] = []
    for i in range(1, args.pairs + 1):
        first = "baseline" if i % 2 == 1 else "after"
        order = ((base_exe, "baseline"), (after_exe, "after"))
        if first == "after":
            order = (order[1], order[0])
        runs = []
        for exe, side in order:
            gpu = query_gpu() or gpu
            runs.append(
                run_once(
                    exe,
                    case,
                    side=side,
                    pair=i,
                    first=first,
                    use_json=use_json,
                    profile=args.profile,
                    gpu=gpu,
                    extra_args=case.get("extra_args"),
                    timeout=args.timeout,
                )
            )
        by_side = {r.side: r for r in runs}
        b, a = by_side["baseline"], by_side["after"]
        delta = (b.wall_s - a.wall_s) / b.wall_s * 100.0
        pairs.append(
            {
                "pair": i,
                "first": first,
                "baseline": b.__dict__,
                "after": a.__dict__,
                "delta_pct": delta,
            }
        )
        print(
            f"  pair {i} ({first} first): "
            f"base={b.wall_s:.3f}s after={a.wall_s:.3f}s delta={delta:+.2f}%"
        )

    base_runs = [p["baseline"] for p in pairs]
    after_runs = [p["after"] for p in pairs]
    base_med = median([r["wall_s"] for r in base_runs])
    after_med = median([r["wall_s"] for r in after_runs])
    delta_pct = (base_med - after_med) / base_med * 100.0

    hashes = {r["fnv1a64"] for r in base_runs + after_runs}
    bit_exact = len(hashes) == 1
    pair_deltas = [p["delta_pct"] for p in pairs]
    same_sign = all(d > 0 for d in pair_deltas) or all(d < 0 for d in pair_deltas)

    if not bit_exact and not args.allow_hash_diff:
        verdict = "hash_mismatch"
    elif delta_pct < -args.gate_pct:
        verdict = "regression"
    elif delta_pct > args.gate_pct:
        verdict = "improvement"
    else:
        verdict = "ok"

    def side_summary(runs: list[dict]) -> dict:
        return {
            "wall_s_median": median([r["wall_s"] for r in runs]),
            "realtime_median": median([r["realtime"] for r in runs]),
            "wall_s_all": [r["wall_s"] for r in runs],
            "fnv1a64": sorted({r["fnv1a64"] for r in runs}),
            "voices_peak_max": max((r["voices_peak"] or 0) for r in runs),
            "voice_frames_per_s_median": median(
                [r["voice_frames_per_s"] for r in runs if r["voice_frames_per_s"]]
            ),
            "sample_gpu_bytes_max": max((r["sample_gpu_bytes"] or 0) for r in runs),
            "sample_cpu_bytes_max": max((r["sample_cpu_bytes"] or 0) for r in runs),
            "phases_us_median": {
                k: median([r["phases"][k] for r in runs if k in r["phases"]])
                for k in PHASES
                if any(k in r["phases"] for r in runs)
            },
            "gpu": gpu_stamp(gpu),
        }

    return {
        "name": case["name"],
        "midi": case["midi"],
        "sf2": case["sf2"],
        "frames": case["frames"],
        "block": case["block"],
        "channels": case.get("channels"),
        "auto_channels": bool(case.get("auto_channels")),
        "extra_args": case.get("extra_args", []),
        "pairs": pairs,
        "baseline": side_summary(base_runs),
        "after": side_summary(after_runs),
        "delta_pct": delta_pct,
        "same_sign": same_sign,
        "bit_exact": bit_exact,
        "verdict": verdict,
    }


def run_gating(
    gating: dict,
    args: argparse.Namespace,
    gpu: dict,
    use_json: bool,
) -> dict:
    """负向门控：窄通道必须丢弃端口 1 且不 panic；宽/自动通道一致。"""
    exe = Path(args.after if args.gating_side == "after" else args.baseline).resolve()
    results: dict[str, Run] = {}
    checks: list[dict] = []

    def add(name: str, ok: bool, detail: str) -> None:
        checks.append({"name": name, "ok": ok, "detail": detail})

    def run(narrow: int | None, auto: bool) -> Run:
        case = dict(gating)
        case.pop("channels", None)
        case.pop("auto_channels", None)
        if auto:
            case["auto_channels"] = True
        else:
            case["channels"] = narrow
        case.setdefault("prewarm", False)
        return run_once(
            exe,
            case,
            side="gating",
            pair=0,
            first="gating",
            use_json=use_json,
            profile=False,
            gpu=gpu,
            timeout=args.timeout,
        )

    narrow = run(gating["narrow_channels"], auto=False)
    wide = run(gating["wide_channels"], auto=False)
    auto = run(None, auto=True)
    # 预热路径单独跑一次：旧实现在 warm_voice_templates 里会越界 panic。
    case = dict(gating)
    case.pop("channels", None)
    case["channels"] = gating["narrow_channels"]
    case["prewarm"] = True
    prewarmed = run_once(
        exe,
        case,
        side="gating",
        pair=0,
        first="gating",
        use_json=use_json,
        profile=False,
        gpu=gpu,
        timeout=args.timeout,
    )

    add("narrow_ok", narrow.returncode == 0, f"rc={narrow.returncode}")
    add("wide_ok", wide.returncode == 0, f"rc={wide.returncode}")
    add("auto_ok", auto.returncode == 0, f"rc={auto.returncode}")
    add(
        "narrow_differs_from_wide",
        narrow.fnv1a64 != wide.fnv1a64,
        f"narrow={narrow.fnv1a64} wide={wide.fnv1a64}",
    )
    add(
        "auto_equals_wide",
        auto.fnv1a64 == wide.fnv1a64,
        f"auto={auto.fnv1a64} wide={wide.fnv1a64}",
    )
    add(
        "narrow_warns_drop",
        "event dropped" in narrow.stderr_tail,
        "stderr 应包含 `event dropped`（每进程一次）",
    )
    add(
        "narrow_prewarm_no_panic",
        prewarmed.returncode == 0 and "panicked" not in prewarmed.stderr_tail,
        f"rc={prewarmed.returncode}",
    )
    add(
        "no_panics",
        all(
            "panicked" not in r.stderr_tail
            for r in (narrow, wide, auto, prewarmed)
        ),
        "任何一次运行都不得 panic",
    )

    results = {
        "narrow": narrow.__dict__,
        "wide": wide.__dict__,
        "auto": auto.__dict__,
        "prewarmed_narrow": prewarmed.__dict__,
    }
    passed = all(c["ok"] for c in checks)
    return {"passed": passed, "checks": checks, "runs": results}


# ---------------------------------------------------------------------------
# 输出
# ---------------------------------------------------------------------------


def write_csv(path: Path, case_reports: list[dict]) -> None:
    fields = [
        "case",
        "pair",
        "first",
        "side",
        "wall_s",
        "realtime",
        "fnv1a64",
        "voices_peak",
        "voice_frames",
        "voice_frames_per_s",
        "sample_gpu_bytes",
        "sample_cpu_bytes",
        "gpu_sm_mhz",
        "gpu_temp_c",
        "gpu_power_w",
        "returncode",
    ]
    with path.open("w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, fieldnames=fields)
        w.writeheader()
        for case in case_reports:
            for pair in case["pairs"]:
                for side in ("baseline", "after"):
                    r = pair[side]
                    w.writerow(
                        {
                            "case": case["name"],
                            "pair": pair["pair"],
                            "first": pair["first"],
                            "side": side,
                            "wall_s": f"{r['wall_s']:.6f}",
                            "realtime": f"{r['realtime']:.3f}",
                            "fnv1a64": r["fnv1a64"],
                            "voices_peak": r["voices_peak"] or "",
                            "voice_frames": r["voice_frames"] or "",
                            "voice_frames_per_s": (
                                f"{r['voice_frames_per_s']:.1f}"
                                if r["voice_frames_per_s"]
                                else ""
                            ),
                            "sample_gpu_bytes": r["sample_gpu_bytes"] or "",
                            "sample_cpu_bytes": r["sample_cpu_bytes"] or "",
                            "gpu_sm_mhz": r["gpu"].get("sm_mhz", ""),
                            "gpu_temp_c": r["gpu"].get("temp_c", ""),
                            "gpu_power_w": r["gpu"].get("power_w", ""),
                            "returncode": r["returncode"],
                        }
                    )


def print_table(case_reports: list[dict]) -> None:
    print("\n=== A/B 汇总（中位数）===")
    print(f"{'case':32} {'base(s)':>9} {'after(s)':>9} {'delta':>8}  verdict")
    print("-" * 78)
    for c in case_reports:
        print(
            f"{c['name']:32} {c['baseline']['wall_s_median']:9.3f} "
            f"{c['after']['wall_s_median']:9.3f} "
            f"{c['delta_pct']:+7.2f}%  {c['verdict']}"
        )

    print("\n=== 分相中位数（us，仅列有数据的相）===")
    for c in case_reports:
        phases = c["baseline"]["phases_us_median"]
        if not phases:
            continue
        print(f"{c['name']}:")
        header = "  " + " ".join(f"{k:>9}" for k in phases)
        print(header)
        base_line = "  " + " ".join(
            f"{c['baseline']['phases_us_median'][k]:9.0f}" for k in phases
        )
        after_line = "  " + " ".join(
            f"{c['after']['phases_us_median'][k]:9.0f}" for k in phases
        )
        delta_line = "  " + " ".join(
            f"{(c['after']['phases_us_median'][k] - phases[k]) / phases[k] * 100:8.1f}%"
            if phases[k]
            else " " * 9
            for k in phases
        )
        print(f"{base_line}   <- baseline")
        print(f"{after_line}   <- after")
        print(f"{delta_line}   <- delta")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", required=True, help="baseline 可执行文件")
    parser.add_argument("--after", required=True, help="after 可执行文件")
    parser.add_argument("--cases", default=str(DEFAULT_CASES))
    parser.add_argument("--label", default="ab")
    parser.add_argument("--pairs", type=int, default=6)
    parser.add_argument("--warmup", type=int, default=1, help="每侧丢弃的预热次数")
    parser.add_argument(
        "--gate-pct",
        type=float,
        default=10.0,
        help="回归判定阈值（默认 10；A/B 精测可用 2）",
    )
    parser.add_argument("--only", default="", help="只跑指定用例（逗号分隔）")
    parser.add_argument("--profile", dest="profile", action="store_true", default=True)
    parser.add_argument("--no-profile", dest="profile", action="store_false")
    parser.add_argument(
        "--allow-hash-diff",
        action="store_true",
        help="允许两侧样本哈希不同（如 REF-LGS-007 量化）",
    )
    parser.add_argument(
        "--gating",
        dest="gating",
        action="store_true",
        default=True,
        help="运行负向通道门控用例（默认开启）",
    )
    parser.add_argument("--no-gating", dest="gating", action="store_false")
    parser.add_argument(
        "--gating-side",
        choices=("after", "baseline"),
        default="after",
        help="门控用例用哪一侧的二进制（默认 after）",
    )
    parser.add_argument("--json", default="", help="汇总 JSON 输出路径")
    parser.add_argument("--csv", default="", help="逐次运行 CSV 输出路径")
    parser.add_argument("--manifest", default=str(DEFAULT_MANIFEST))
    parser.add_argument("--timeout", type=int, default=1800)
    parser.add_argument(
        "--fail-on",
        choices=("none", "regression", "any"),
        default="none",
        help="退出码策略（默认 none；CI 用 any）",
    )
    args = parser.parse_args()

    base_exe = Path(args.baseline).resolve()
    after_exe = Path(args.after).resolve()
    for exe in (base_exe, after_exe):
        if not exe.is_file():
            raise SystemExit(f"找不到可执行文件: {exe}")

    cases_data = load_cases(Path(args.cases))
    manifest = {}
    manifest_path = Path(args.manifest)
    if manifest_path.is_file():
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))

    use_json = binary_supports(base_exe, "--json") and binary_supports(
        after_exe, "--json"
    )
    if not use_json:
        print("[warn] after 二进制不支持 --json，退回 stdout 解析")
    if args.gating and not binary_supports(after_exe, "--channels"):
        print("[warn] after 二进制不支持 --channels，跳过负向门控")
        args.gating = False

    gpu = query_gpu()
    started_at = datetime.now(timezone.utc).astimezone()
    t_start = time.perf_counter()

    selected = cases_data["cases"]
    if args.only:
        wanted = {x.strip() for x in args.only.split(",") if x.strip()}
        selected = [c for c in selected if c["name"] in wanted]
        if not selected:
            raise SystemExit(f"--only 无匹配用例: {args.only}")

    # 夹具登记校验：防止"跑了半天发现夹具换了"。
    errors = []
    checked: set[str] = set()
    for case in selected:
        for key in ("midi", "sf2"):
            p = REPO_ROOT / case[key]
            if p.name in checked or not p.exists():
                continue
            checked.add(p.name)
            if manifest:
                err = verify_fixture(p, manifest)
                if err:
                    errors.append(err)
    if errors:
        raise SystemExit("夹具校验失败：\n  " + "\n  ".join(errors))

    print(
        f"baseline={base_exe.name} after={after_exe.name} "
        f"pairs={args.pairs} gate={args.gate_pct}% json={use_json}"
    )
    if gpu:
        print(
            f"GPU: {gpu.get('name')} driver={gpu.get('driver')} "
            f"sm={gpu.get('sm_mhz')}MHz temp={gpu.get('temp_c')}C"
        )

    case_reports = []
    for case in selected:
        print(f"\n=== case: {case['name']} ===")
        report = run_case(case, args, gpu, use_json)
        report["fixtures"] = {
            key: {
                "path": case[key],
                "size": (REPO_ROOT / case[key]).stat().st_size,
                "sha256": sha256_file(REPO_ROOT / case[key]),
            }
            for key in ("midi", "sf2")
        }
        case_reports.append(report)

    gating_report = None
    if args.gating and cases_data.get("gating"):
        print("\n=== 负向通道门控 ===")
        gating_report = run_gating(cases_data["gating"], args, gpu, use_json)
        for check in gating_report["checks"]:
            mark = "PASS" if check["ok"] else "FAIL"
            print(f"  [{mark}] {check['name']}: {check['detail']}")

    print_table(case_reports)

    verdicts = [c["verdict"] for c in case_reports]
    if gating_report and not gating_report["passed"]:
        verdicts.append("gating_failed")
    if "hash_mismatch" in verdicts:
        overall = "hash_mismatch"
    elif "gating_failed" in verdicts:
        overall = "gating_failed"
    elif "regression" in verdicts:
        overall = "regression"
    elif "improvement" in verdicts:
        overall = "improvement"
    else:
        overall = "ok"

    summary = {
        "label": args.label,
        "started_at": started_at.isoformat(timespec="seconds"),
        "duration_s": round(time.perf_counter() - t_start, 3),
        "host": {
            "platform": platform.platform(),
            "python": sys.version.split()[0],
            "cpu_count": os.cpu_count(),
        },
        "gpu": gpu,
        "baseline": {
            "path": str(base_exe),
            "size": base_exe.stat().st_size,
            "sha256": sha256_file(base_exe),
            "mtime": datetime.fromtimestamp(base_exe.stat().st_mtime).isoformat(
                timespec="seconds"
            ),
        },
        "after": {
            "path": str(after_exe),
            "size": after_exe.stat().st_size,
            "sha256": sha256_file(after_exe),
            "mtime": datetime.fromtimestamp(after_exe.stat().st_mtime).isoformat(
                timespec="seconds"
            ),
        },
        "config": {
            "pairs": args.pairs,
            "warmup": args.warmup,
            "gate_pct": args.gate_pct,
            "profile": args.profile,
            "cases_file": str(Path(args.cases).resolve()),
        },
        "cases": case_reports,
        "gating": gating_report,
        "verdict": overall,
    }

    if args.json:
        out = Path(args.json)
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(
            json.dumps(summary, indent=2, ensure_ascii=False) + "\n",
            encoding="utf-8",
        )
        print(f"\njson: {out}")
    if args.csv:
        out = Path(args.csv)
        out.parent.mkdir(parents=True, exist_ok=True)
        write_csv(out, case_reports)
        print(f"csv: {out}")

    print(f"\nVERDICT: {overall}")

    if args.fail_on == "any":
        return 0 if overall == "ok" else 1
    if args.fail_on == "regression":
        return 0 if overall in ("ok", "improvement") else 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
