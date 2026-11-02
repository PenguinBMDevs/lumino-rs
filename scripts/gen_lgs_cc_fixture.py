#!/usr/bin/env python3
"""生成 REF-LGS-004 的高事件密度夹具（标准库实现，零依赖）。

目的：isolate **mix 事件重放**成本。夹具只持有少量持续音（低 GPU 声部成本），
同时以极高密度灌入通道控制器（CC7 音量 / CC11 表情 / CC10 声像），使每个
渲染块的事件数达到黑乐谱密集段的量级（≈2k 事件/块 @ block 512 / 48 kHz）。

产物默认写入 ``test-file/bench/``（gitignored），并写
``cc_storm_manifest.json``（尺寸 + SHA256 + 每块事件数估算）。

用法::

    python scripts/gen_lgs_cc_fixture.py
    python scripts/gen_lgs_cc_fixture.py --duration 20 --events-per-tick 400

设计要点：

- 120 BPM / division 480 → 960 tick/s；block 512 @ 48 kHz = 10.67 ms/块。
- 保持音：每通道 1 个长音（16 声部），确保 mix pass 有输出但不被声部 gather 淹没。
- 事件完全确定性（无随机数），同一组参数生成逐字节相同的文件。
- 所有 CC 事件 delta=0 追加在同一 tick，事件帧号相同——这正是最坏情况：
  每帧线程都要扫过本块「已发生」的全部事件。
"""

from __future__ import annotations

import argparse
import hashlib
import json
import struct
from pathlib import Path

BPM = 120
DIVISION = 480
TICKS_PER_SECOND = DIVISION * BPM // 60  # 960
SAMPLE_RATE = 48_000
BLOCK = 512

# CC7 音量 / CC11 表情 / CC10 声像：mix 内核真正重放的三类控制器。
CCS = (7, 11, 10)


def varint(value: int) -> bytes:
    if value < 0:
        raise ValueError("varint 不接受负数")
    out = bytearray([value & 0x7F])
    value >>= 7
    while value:
        out.insert(0, (value & 0x7F) | 0x80)
        value >>= 7
    return bytes(out)


def track_chunk(events: list[tuple[int, int, bytes]]) -> bytes:
    """(tick, order, bytes) → MTrk。order 保证同 tick 内先 meta/program 后 CC。"""
    events.sort(key=lambda e: (e[0], e[1]))
    body = bytearray()
    last = 0
    for tick, _order, data in events:
        body += varint(tick - last)
        body += data
        last = tick
    body += varint(0) + b"\xff\x2f\x00"
    return b"MTrk" + struct.pack(">I", len(body)) + bytes(body)


def make_cc_storm(duration_s: int, events_per_tick: int) -> tuple[bytes, dict]:
    ticks = duration_s * TICKS_PER_SECOND
    tracks = [
        track_chunk([(0, 0, b"\xff\x51\x03" + (500_000).to_bytes(3, "big"))])
    ]

    for ch in range(16):
        events: list[tuple[int, int, bytes]] = [
            (0, 1, bytes([0xC0 | ch, 0])),
            (0, 3, bytes([0x90 | ch, 48 + ch, 100])),
            (ticks, 2, bytes([0x80 | ch, 48 + ch, 0])),
        ]
        # 每个 tick 均匀铺 `events_per_tick` 条 CC，轮转通道与控制器号。
        for t in range(ticks):
            for k in range(events_per_tick):
                if (k % 16) != ch:
                    continue
                cc = CCS[(k // 16) % len(CCS)]
                # 确定性三角波 0..127，避免长期贴在端点（更接近真实自动化）。
                phase = (t * 7 + k * 13) % 254
                value = phase if phase < 127 else 254 - phase
                events.append((t, 4, bytes([0xB0 | ch, cc, value])))
        tracks.append(track_chunk(events))

    header = b"MThd" + struct.pack(">IHHH", 6, 1, len(tracks), DIVISION)
    data = header + b"".join(tracks)

    per_block = events_per_tick * BLOCK / SAMPLE_RATE * TICKS_PER_SECOND
    meta = {
        "duration_s": duration_s,
        "events_per_tick": events_per_tick,
        "events_total": duration_s * TICKS_PER_SECOND * events_per_tick,
        "events_per_block_est": round(per_block, 1),
        "block": BLOCK,
        "sample_rate": SAMPLE_RATE,
    }
    return data, meta


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest().upper()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out-dir", default="test-file/bench")
    parser.add_argument("--duration", type=int, default=10, help="音频时长（秒，默认 10）")
    parser.add_argument(
        "--events-per-tick",
        type=int,
        default=200,
        help="每 tick 的 CC 事件数（默认 200 → ≈2.1k 事件/块 @512）",
    )
    args = parser.parse_args()

    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    data, meta = make_cc_storm(args.duration, args.events_per_tick)
    name = f"cc_storm_{args.duration}s_{args.events_per_tick}ept.mid"
    (out_dir / name).write_bytes(data)
    manifest = {
        "generator": "scripts/gen_lgs_cc_fixture.py",
        "version": 1,
        "bpm": BPM,
        "division": DIVISION,
        "file": name,
        "size": len(data),
        "sha256": sha256(data),
        **meta,
    }
    (out_dir / "cc_storm_manifest.json").write_text(
        json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )
    print(
        f"{name}: {len(data)} B sha256={sha256(data)} "
        f"events≈{meta['events_total']} (~{meta['events_per_block_est']}/block)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
