//! GPU 基准测试（#[ignore]；从 perf_tests.rs 拆出）

use super::frame_paths::*;
use super::helpers::*;
use super::measure::*;
use super::*;

/// PREF-005 A/B：混合+compute cull（改动前） vs 不透明+compute cull vs 不透明+VS cull。
///
/// 验收口径即本表的 `direct / blended - 1`：卡内目标是 zoom-out 全曲视图
/// **帧时间下降 ≥ 50%**。
#[test]
#[ignore = "GPU 基准：需显式 --ignored 运行，见模块头注释"]
fn bench_vs_cull_ab() {
    let gpu = BenchGpu::new();
    let counts = env_counts(&gpu);
    let iters = env_iters();
    assert!(!counts.is_empty(), "无可测档位（设备容量过小？）");

    for (view, zoomed) in [
        ("全景（全曲缩放到屏）", false),
        // zoom-in：cull 剔除率最高的档位（PREF-004 实测该档 cull 占总帧 56%）。
        // 本卡验收要求「zoom-in 渲染帧时间不回退」——直绘取消了 cull pass，
        // 该档预期为收益；此处即为该条验收的实测来源。
        ("放大（局部约 52 键）", true),
    ] {
        for (scene, dense) in [
            ("dense（同 key 堆叠 · 黑乐谱形态）", true),
            ("uniform（均匀铺满）", false),
        ] {
            eprintln!(
                "\n=== {view} · {scene} · adapter={} · {iters} 轮中位数（前 2 轮预热丢弃）===",
                gpu.adapter_info
            );
            eprintln!(
                "{:>12} {:>12} {:>12} {:>12} {:>10} {:>10}",
                "notes", "blended+cull", "opaque+cull", "opaque+直接", "去混合", "总降幅"
            );
            for &count in &counts {
                let (notes, span) = if dense {
                    synth_notes_dense(count)
                } else {
                    (synth_notes(count), TOTAL_TICKS)
                };
                let camera = if zoomed {
                    // 放大档：2 px/tick × 20 px/key（约 52 键可见）
                    CameraUniform {
                        zoom: [2.0, 20.0],
                        ..panorama_camera_for(span)
                    }
                } else {
                    panorama_camera_for(span)
                };
                let [blended, cull, direct] = measure_frame_paths(&gpu, &notes, camera, iters);
                eprintln!(
                    "{:>12} {:>11.2} {:>11.2} {:>11.2} {:>9.0}% {:>9.0}%",
                    count,
                    blended,
                    cull,
                    direct,
                    (1.0 - cull / blended.max(f64::EPSILON)) * 100.0,
                    (1.0 - direct / blended.max(f64::EPSILON)) * 100.0
                );
            }
        }
    }
}
///
/// 全景（缩小到底：全曲 tick + 全键可见）分段耗时。
///
/// 这是黑乐谱滚动拖拽的**主场景**：cull 几乎不剔除任何音符，
/// 且全部 quad 都是亚像素宽。
#[test]
#[ignore = "GPU 基准：需显式 --ignored 运行，见模块头注释"]
fn bench_note_layer_panorama_gpu_breakdown() {
    let gpu = BenchGpu::new();
    let counts = env_counts(&gpu);
    let iters = env_iters();
    assert!(!counts.is_empty(), "无可测档位（设备容量过小？）");

    print_header(&gpu, "panorama（缩小全景）", iters);
    let mut rows = Vec::new();
    for count in counts {
        let row = measure(&gpu, count, false, iters);
        print_row(&row);
        rows.push(row);
    }

    // 趋势说明：打印 ns/实例，用于判断 cull 是否线性于总音符数
    eprintln!("\n[perf] panorama 单位成本：");
    for r in &rows {
        let n = r.count as f64;
        eprintln!(
            "  notes={:>10}  cull={:>7.3} ns/inst   draw(可见)={:>7.3} ns/visible   draw(总)={:>7.3} ns/inst   point={:>7.3} ns/inst   提速={:>5.2}x",
            r.count,
            r.cull_ms * 1e6 / n,
            r.draw_ms * 1e6 / r.visible.max(1) as f64,
            r.draw_ms * 1e6 / n,
            r.point_ms * 1e6 / n,
            if r.point_ms > 0.0 {
                r.total_ms / r.point_ms
            } else {
                0.0
            }
        );
    }
}

/// 局部放大（约 0.06% tick × 52 键可见）分段耗时——cull 剔除率最高的场景。
#[test]
#[ignore = "GPU 基准：需显式 --ignored 运行，见模块头注释"]
fn bench_note_layer_zoomed_gpu_breakdown() {
    let gpu = BenchGpu::new();
    let counts = env_counts(&gpu);
    let iters = env_iters();
    assert!(!counts.is_empty(), "无可测档位（设备容量过小？）");

    print_header(&gpu, "zoomed（局部放大）", iters);
    for count in counts {
        print_row(&measure(&gpu, count, true, iters));
    }
}
