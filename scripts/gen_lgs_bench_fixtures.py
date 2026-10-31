#!/usr/bin/env python3
"""生成 REF-LGS-013 多端口基准夹具（标准库实现，零依赖）。

覆盖档位：16 / 32 / 64 / 128 / 256 通道（1/2/4/8/16 端口）。

产物默认写入 ``test-file/bench/``（该目录被 .gitignore 排除，不入库），
同时写入 ``multiport_manifest.json``（尺寸 + SHA256），供 lgs_ab.py 校验。

用法::

    python scripts/gen_lgs_bench_fixtures.py
    python scripts/gen_lgs_bench_fixtures.py --out-dir test-file/bench \
        --duration 30 --notes-per-channel 8

设计要点：

- SMF format 1：1 条 tempo 轨 + 每端口 1 条轨，每条端口轨先写 ``FF 21``
  MidiPort 元事件（与 LGS 解析器的 ``track_ports`` 语义一致）。
- 每端口使用不同的音高区间，因此"只有端口 0 被渲染"（通道配置过窄）与
  "全部端口被渲染"的输出哈希必然不同 —— 负向门控用例依赖这一点。
- 120 BPM / division 480 / 48 kHz → 1 tick = 50 样本，时间轴为整数。
- 无随机数：同一组参数永远生成逐字节相同的文件。
"""

from __future__ import annotations

import argparse
import hashlib
import json
import struct
from pathlib import Path

# 120 BPM -> 500000 us/quarter；division 480 -> 960 ticks/s。
BPM = 120
DIVISION = 480
TICKS_PER_SECOND = DIVISION * BPM // 60
SAMPLE_RATE = 48_000

PORT_TIERS = (1, 2, 4, 8, 16)


def varint(value: int) -> bytes:
    """SMF 可变长整数（delta time / meta 长度）。"""
    if value < 0:
        raise ValueError("varint 不接受负数")
    out = bytearray([value & 0x7F])
    value >>= 7
    while value:
        out.insert(0, (value & 0x7F) | 0x80)
        value >>= 7
    return bytes(out)


def track_chunk(events: list[tuple[int, int, bytes]]) -> bytes:
    """把 (tick, order, bytes) 事件序列编码为一个 MTrk 块。

    ``order`` 用于同一 tick 内的稳定排序：0=meta，1=program，2=note-off，
    3=note-on（先关后开，避免同 tick 的 NoteOn 被 NoteOff 立刻收掉）。
    """
    events.sort(key=lambda e: (e[0], e[1]))
    body = bytearray()
    last_tick = 0
    for tick, _order, data in events:
        body += varint(tick - last_tick)
        body += data
        last_tick = tick
    body += varint(0) + b"\xff\x2f\x00"  # End of Track
    return b"MTrk" + struct.pack(">I", len(body)) + bytes(body)


def make_multiport(ports: int, duration_s: int, notes_per_channel: int) -> bytes:
    """生成 ``ports`` 个端口、每端口 16 通道的持续音夹具。"""
    duration_ticks = duration_s * TICKS_PER_SECOND
    # 持续 60% 时长：错峰进入后形成稳定的高重叠区间。
    sustain_ticks = max(duration_ticks * 3 // 5, 1)
    spread = max(duration_ticks - sustain_ticks - 1, 1)

    tempo_track = track_chunk(
        [(0, 0, b"\xff\x51\x03" + (500_000).to_bytes(3, "big"))]
    )

    tracks = [tempo_track]
    for port in range(ports):
        events: list[tuple[int, int, bytes]] = [
            (0, 0, b"\xff\x21\x01" + bytes([port]))
        ]
        base_key = 40 + port * 3  # 端口间音高不重叠，门控可辨别
        for ch in range(16):
            key = base_key + (ch % 12)
            if key > 127:
                key = 127
            # 每个通道先选一次音色，确保端口/通道都被真实使用。
            events.append((0, 1, bytes([0xC0 | ch, 0])))
            for n in range(notes_per_channel):
                start = 1 + (n * spread) // max(notes_per_channel - 1, 1)
                end = min(start + sustain_ticks, duration_ticks)
                vel = 64 + (ch * 3 + n) % 32  # 64..95，确定性
                events.append((start, 3, bytes([0x90 | ch, key, vel])))
                events.append((end, 2, bytes([0x80 | ch, key, 0])))
        tracks.append(track_chunk(events))

    header = b"MThd" + struct.pack(">IHHH", 6, 1, len(tracks), DIVISION)
    return header + b"".join(tracks)


def make_gate(duration_s: int = 4) -> bytes:
    """2 端口最小门控夹具：端口 0 = 低音区，端口 1 = 高音区。

    ``midi_channels=16`` 时端口 1 的事件必须被丢弃（只剩低音区）；
    ``midi_channels=32``（或 --auto-channels）时两个音区都在。
    """
    duration_ticks = duration_s * TICKS_PER_SECOND
    tracks = [
        track_chunk([(0, 0, b"\xff\x51\x03" + (500_000).to_bytes(3, "big"))]),
    ]
    for port, key in ((0, 48), (1, 84)):
        events: list[tuple[int, int, bytes]] = [
            (0, 0, b"\xff\x21\x01" + bytes([port])),
            (0, 1, bytes([0xC0, 0])),
            (1, 3, bytes([0x90, key, 100])),
            (duration_ticks - 1, 2, bytes([0x80, key, 0])),
        ]
        tracks.append(track_chunk(events))
    header = b"MThd" + struct.pack(">IHHH", 6, 1, len(tracks), DIVISION)
    return header + b"".join(tracks)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest().upper()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--out-dir",
        default="test-file/bench",
        help="输出目录（默认 test-file/bench，gitignored）",
    )
    parser.add_argument(
        "--duration",
        type=int,
        default=30,
        help="夹具音频时长（秒，默认 30）",
    )
    parser.add_argument(
        "--notes-per-channel",
        type=int,
        default=8,
        help="每通道持续音数量（默认 8）",
    )
    parser.add_argument(
        "--only",
        default="",
        help="只生成指定档位（逗号分隔的端口数，如 2,4）",
    )
    args = parser.parse_args()

    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    tiers = PORT_TIERS
    if args.only:
        wanted = {int(x) for x in args.only.split(",") if x.strip()}
        tiers = tuple(p for p in PORT_TIERS if p in wanted)
        if not tiers:
            parser.error(f"--only 无匹配档位: {args.only}")

    files: list[dict] = []
    for ports in tiers:
        data = make_multiport(ports, args.duration, args.notes_per_channel)
        name = f"multiport_{ports}p_{ports * 16}ch.mid"
        (out_dir / name).write_bytes(data)
        files.append(
            {
                "name": name,
                "kind": "multiport",
                "ports": ports,
                "channels": ports * 16,
                "frames": args.duration * SAMPLE_RATE,
                "size": len(data),
                "sha256": sha256(data),
            }
        )
        print(f"{name}: {len(data)} B  sha256={sha256(data)}")

    gate = make_gate()
    gate_name = "gate_2p_32ch.mid"
    (out_dir / gate_name).write_bytes(gate)
    files.append(
        {
            "name": gate_name,
            "kind": "gate",
            "ports": 2,
            "channels": 32,
            "frames": 4 * SAMPLE_RATE,
            "size": len(gate),
            "sha256": sha256(gate),
        }
    )
    print(f"{gate_name}: {len(gate)} B  sha256={sha256(gate)}")

    manifest = {
        "generator": "scripts/gen_lgs_bench_fixtures.py",
        "version": 1,
        "sample_rate": SAMPLE_RATE,
        "bpm": BPM,
        "division": DIVISION,
        "duration_s": args.duration,
        "notes_per_channel": args.notes_per_channel,
        "files": files,
    }
    manifest_path = out_dir / "multiport_manifest.json"
    manifest_path.write_text(
        json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )
    print(f"\nmanifest: {manifest_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
