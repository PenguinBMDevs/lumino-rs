# -*- coding: utf-8 -*-
"""本卡复现仿真 — 原样复刻卡片所附程序（用于核对结论）"""
import numpy as np

GPU_BW_GBs   = 400.0
CULL_OVERHEAD_MS = 0.4
PIXEL_RATE_Gpx = 30.0
VSYNC_MS = 16.67

def cull_ms(total):  return total * 16 / (GPU_BW_GBs*1e9)*1000 + CULL_OVERHEAD_MS
def fill_ms(vis, px, od): return vis * px * od / (PIXEL_RATE_Gpx*1e9)*1000

def simulate(total_notes, visible, avg_px, overdraw, wait_coupling=True,
             duration_ms=1000.0, event_hz=125):
    gpu_frame = cull_ms(total_notes) + fill_ms(visible, avg_px, overdraw)
    events = np.arange(0, duration_ms, 1000.0/event_hz)
    gpu_free, starts, dones = 0.0, [], []
    for t in events:
        if t >= gpu_free:
            starts.append(t)
            gpu_free = t + 0.3 + gpu_frame
            dones.append(gpu_free)
    starts, dones = np.array(starts), np.array(dones)
    lat = []
    for t in events:
        if not wait_coupling:
            lat.append(VSYNC_MS); continue
        i = min(np.searchsorted(starts, t, 'left'), len(dones)-1)
        lat.append(np.ceil(dones[i]/VSYNC_MS)*VSYNC_MS - t)
    return np.array(lat), len(dones)/(duration_ms/1000), gpu_frame

if __name__ == "__main__":
    for total, vis, px, od, label in [
        (2_000_000, 400_000, 4, 10, "200万·局部放大"),
        (150_000_000, 3_000_000, 4, 30, "1.5亿·缩小全景"),
        (600_000_000, 12_000_000, 4, 30, "6亿·极限"),
    ]:
        lat, fps, g = simulate(total, vis, px, od)
        print(f"{label:16s} GPU帧 {g:6.1f}ms  画面 {fps:5.0f}FPS  "
              f"UI延迟均值 {lat.mean():6.1f}ms  P95 {np.percentile(lat,95):6.1f}ms  "
              f">33ms {(lat>33).mean()*100:3.0f}%")
    lat2, fps2, _ = simulate(150_000_000, 3_000_000, 4, 30, wait_coupling=False)
    print(f"{'1.5亿·免等待':16s} UI延迟均值 {lat2.mean():6.1f}ms  "
          f"P95 {np.percentile(lat2,95):6.1f}ms  >33ms {(lat2>33).mean()*100:3.0f}%  画面仍 {fps2:.0f}FPS")
