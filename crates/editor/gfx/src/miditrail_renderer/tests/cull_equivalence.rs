//! Miditrail 导出 cull 等价性：`seed_resident` → `cull_window` 回读 ≡ CPU 窗口。
//!
//! 覆盖渲染器自有常驻路径（播种/世代/回读装配），谓词层已由
//! `global_bucket::cull_tests` 证明；此处断言经回读的 compact 与 CPU 参考
//!（同谓词 + **画家序**，见 `super::paint_order_window`）逐字节一致——回读后
//! legacy 渲染像素随之逐位一致（`build_note_instances` 内部稳定排序，输入同序
//! 则输出同序）。

use super::super::{MiditrailRenderer, types::miditrail_viewport_span};
use crate::{CullWindow, NoteInstance};
use futures::executor::block_on;

fn test_device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        .expect("测试需要可用的 wgpu 适配器");
    block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("miditrail_cull_equiv_device"),
        required_features: adapter.features() & wgpu::Features::default(),
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::default(),
        trace: wgpu::Trace::Off,
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
    }))
    .expect("请求 wgpu 设备失败")
}

fn synthetic_full() -> Vec<NoteInstance> {
    let mut state = 0x51F1_5EED_1234_5678u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut notes = Vec::new();
    for i in 0..2500u32 {
        let key = (next() % 128) as u8;
        let start = (next() % 12000) as f32;
        let len = (60 + next() % 2000) as f32;
        let shade = (i % 5) as f32 / 5.0;
        notes.push(NoteInstance::new(
            start,
            key,
            len,
            [0.3 + shade * 0.4, 0.8 - shade * 0.3, 0.4 + shade * 0.2, 1.0],
            0,
        ));
    }
    // 并列组（同 key 同 start 不同长度，cull 与 CPU 同为 load 序）。
    for g in 0..48u32 {
        let key = (g * 53 % 128) as u8;
        let start = 5000.0 + (g % 6) as f32 * 200.0;
        for v in 0..3u32 {
            notes.push(NoteInstance::new(
                start,
                key,
                300.0 + v as f32 * 400.0,
                [0.3 + v as f32 * 0.2, 0.6, 0.8 - v as f32 * 0.15, 1.0],
                0,
            ));
        }
    }
    notes
}

#[test]
fn test_miditrail_cull_window_matches_cpu() {
    const TICK: u32 = 5000;
    const KEY_COUNT: usize = 128;
    // 与生产同公式（ppq=480, speed=1.0, z_far=7.5=SCENE_DEPTH → 全跨度 7680）。
    let tick_end = TICK.saturating_add(miditrail_viewport_span(480, 1.0, 7.5));
    let notes = synthetic_full();
    let expected = super::paint_order_window(&notes, TICK, tick_end, KEY_COUNT);
    assert!(!expected.is_empty(), "合成窗口非空（测试前提）");

    let (device, queue) = test_device();
    let mut renderer = MiditrailRenderer::new(&device);
    renderer.seed_resident(&device, &queue, &notes);
    let (window, _timing) = renderer
        .cull_window(
            &device,
            &queue,
            CullWindow {
                tick_start: TICK,
                tick_end,
                key_count: KEY_COUNT,
            },
        )
        .expect("cull 窗口提取应成功");
    assert_eq!(
        window.len(),
        expected.len(),
        "cull 窗口数量必须与 CPU 参考一致"
    );
    assert_eq!(
        bytemuck::cast_slice::<NoteInstance, u8>(&window),
        bytemuck::cast_slice::<NoteInstance, u8>(&expected),
        "cull 回读必须与 CPU 窗口逐字节一致（含序）"
    );
    renderer.restore_window(window);
}

#[test]
fn test_miditrail_cull_empty_window() {
    // 空窗口（tick 越过全部音符）：返回空集，不回读、不崩溃。
    let notes = synthetic_full();
    let (device, queue) = test_device();
    let mut renderer = MiditrailRenderer::new(&device);
    renderer.seed_resident(&device, &queue, &notes);
    let (window, _timing) = renderer
        .cull_window(
            &device,
            &queue,
            CullWindow {
                tick_start: 9_000_000,
                tick_end: 9_010_000,
                key_count: 128,
            },
        )
        .expect("空窗口 cull 应成功");
    assert!(window.is_empty(), "越过全曲的窗口必须为空");
    renderer.restore_window(window);
}

/// GPU 活跃键聚合（COUNT 顺带）≡ CPU `compute_active_and_aura_for_compact`。
///
/// CPU 参考输入取 bucket 序（key 主序、start 次序、并列 load 序稳定）——
/// 与全局桶 GPU 稳定排序语义一致，保证"最后一个覆盖者取色"逐键同值。
/// 光晕系数含 `powf(0.3)`，CPU/GPU 有 ULP 级差异，按 1e-5 容差断言。
#[test]
fn test_cull_active_matches_cpu_reference() {
    use super::super::cull::decode_active_for_gpu;
    use super::super::instances::compute_active_and_aura_for_compact;

    const TICK: u32 = 5000;
    const KEY_COUNT: usize = 128;
    const TPS: f32 = 960.0;
    const FPS: f32 = 60.0;
    let tick_end = TICK.saturating_add(miditrail_viewport_span(480, 1.0, 7.5));
    let notes = synthetic_full();

    let mut ordered = notes.clone();
    ordered.sort_by(|a, b| {
        let ka = a.key_color & 0xFF;
        let kb = b.key_color & 0xFF;
        ka.cmp(&kb).then_with(|| {
            (a.start_length[0].max(0.0) as u32).cmp(&(b.start_length[0].max(0.0) as u32))
        })
    });
    let (expected_keys, expected_aura) =
        compute_active_and_aura_for_compact(TICK, TPS, FPS, &ordered);
    assert!(
        expected_keys.pressed.iter().any(|p| *p),
        "合成场景应有按下键（测试前提）"
    );

    let (device, queue) = test_device();
    let mut renderer = MiditrailRenderer::new(&device);
    renderer.seed_resident(&device, &queue, &notes);
    let prepared = renderer
        .cull_prepare(
            &device,
            &queue,
            CullWindow {
                tick_start: TICK,
                tick_end,
                key_count: KEY_COUNT,
            },
            crate::CullActiveParams {
                ticks_per_second: TPS,
                fps: FPS,
            },
        )
        .expect("cull_prepare 应成功");
    assert!(prepared.total > 0, "窗口非空（测试前提）");

    let (got_keys, got_aura) = decode_active_for_gpu(&prepared.active, KEY_COUNT);
    assert_eq!(
        got_keys.pressed, expected_keys.pressed,
        "pressed 必须逐键一致"
    );
    assert_eq!(got_keys.colors, expected_keys.colors, "键色必须逐键一致");
    for k in 0..128 {
        let (a, b) = (got_aura[k], expected_aura[k]);
        assert!(
            (a - b).abs() <= 1e-5,
            "第 {k} 键光晕系数超容差：gpu={a} cpu={b}"
        );
    }
}
