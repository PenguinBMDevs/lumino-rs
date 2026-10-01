# -*- coding: utf-8 -*-
"""
卡片复现仿真的审计版：分离“模型假设”与“结论”，并检验“修复后”一行是否可由模型推出。
输出使用 ASCII 标签以避免控制台编码问题。
"""
import numpy as np

GPU_BW_GBs = 400.0
CULL_OVERHEAD_MS = 0.4
PIXEL_RATE_Gpx = 30.0
VSYNC_MS = 16.67
ENC_MS = 0.3


def cull_ms(total):
    return total * 16 / (GPU_BW_GBs * 1e9) * 1000 + CULL_OVERHEAD_MS


def fill_ms(vis, px, od):
    return vis * px * od / (PIXEL_RATE_Gpx * 1e9) * 1000


def sim(total, vis, px, od, vsync_quantize=True, duration_ms=1000.0, hz=125):
    """卡片模型：UI 不阻塞（理想解耦），事件由“第一个在 t 之后启动的帧”服务。"""
    T = cull_ms(total) + fill_ms(vis, px, od)
    events = np.arange(0, duration_ms, 1000.0 / hz)
    starts, dones, free = [], [], 0.0
    for t in events:
        if t >= free:
            starts.append(t)
            free = t + ENC_MS + T
            dones.append(free)
    starts, dones = np.array(starts), np.array(dones)
    lat = []
    for t in events:
        i = min(np.searchsorted(starts, t, 'left'), len(dones) - 1)
        d = dones[i]
        if vsync_quantize:
            d = np.ceil(d / VSYNC_MS) * VSYNC_MS
        lat.append(d - t)
    return np.array(lat), T


def report(name, lat, T):
    print(f"{name:<38s} T={T:6.2f}ms  mean={lat.mean():6.1f}  "
          f"P95={np.percentile(lat,95):6.1f}  >33ms={(lat>33).mean()*100:3.0f}%")


print("=== A. 卡片基线（含 vsync 量化）===")
lat, T = sim(150_000_000, 3_000_000, 4, 30)
report("1.5e8 panorama (card baseline)", lat, T)
print("    卡片宣称的修复后 : mean=16.7  P95=16.7  >33ms=0%   <- 模型中不存在能产生该结果的路径")

print("\n=== B. 同一模型、同一 T，去掉 vsync 量化（Windows 实际选 Immediate）===")
lat_b, _ = sim(150_000_000, 3_000_000, 4, 30, vsync_quantize=False)
report("1.5e8 panorama (Immediate)", lat_b, T)
print("    => 卡片表格里 48.6ms 的 P95 有最多 16.7ms 来自 'ceil to vsync'，"
      "而 context.rs 在非 macOS 选的是 Immediate")

print("\n=== C. 反推达标所需的 GPU 单帧 T（理想解耦下 latency 落在 [T, 2T]）===")
for target in (33.0, 24.0):
    lo, hi = 1.0, 200.0
    for _ in range(60):
        mid = (lo + hi) / 2
        # 直接构造 [T,2T] 的均匀到达延迟分布
        e = np.arange(0, 1000.0, 1.0)
        lat_m = np.array([mid + (e_i % mid) for e_i in (e % mid)])
        lat_m = np.array([mid * 2 - (x - mid) for x in lat_m])  # 保持保守
        lat_m = np.linspace(mid, 2 * mid, 4000)
        if np.percentile(lat_m, 95) > target:
            hi = mid
        else:
            lo = mid
    print(f"    P95 <= {target:.0f}ms  ==>  T <= ~{(lo+hi)/2:.1f}ms")

print("\n=== D. T 的构成与各候选优化能砍掉多少 ===")
total, vis, px, od = 150_000_000, 3_000_000, 4, 30
c = cull_ms(total)
f = fill_ms(vis, px, od)
print(f"    cull({total:,})            = {c:6.2f} ms")
print(f"    fill({vis:,},{px}px,od{od}) = {f:6.2f} ms")
print(f"    T                        = {c+f:6.2f} ms")
print(f"    VS 二次读取同一 16B/可见实例 = {vis*16/400e9*1000:6.2f} ms (全景时 = cull 同量级)")
print("\n    注：sim 的 fill 用像素率 30Gpx/s；对亚像素 quad(1.5e8 个)真实瓶颈通常是"
      "\n    primitive setup / 小三角形光栅吞吐（Gtri/s），量纲不同，不可用 Gpx/s 外推。")

print("\n=== E. 敏感性：全景下 visible 与 overdraw 的不确定性 ===")
for vis2, od2 in ((3e6, 30), (3e6, 100), (3e7, 30), (1.5e8, 30)):
    T2 = cull_ms(total) + fill_ms(vis2, od2 and 4, od2)
    l2, _ = sim(total, vis2, 4, od2)
    report(f"vis={vis2:>10,.0f} od={od2:<4d}", l2, T2)
