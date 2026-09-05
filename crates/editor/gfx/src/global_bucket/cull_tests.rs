//! cull 窗口提取集合等价性：GPU 两阶段提取 ≡ CPU 窗口收集 + 排序。
//!
//! - `test_cull_window_set_equals_cpu_window`（严格）：合成全集经 cull 提取的
//!   compact 与 CPU 参考（同谓词过滤 + `(key, start)` 稳定排序）逐字节一致；
//! - `test_cull_finds_buried_long_note`（召回）：300+ 死音符之下的覆盖长音必须
//!   被提取（旧 `waterfall_indexed.wgsl` SEARCH_BUFFER=128 回溯在此例漏检，
//!   是导出改走 cull 的直接原因，见 `bucket_cull.wgsl` 头注）。

use super::{ResidentCull, prefix_counts};
use crate::{CullWindow, NoteInstance};
use futures::executor::block_on;

fn test_device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        .expect("测试需要可用的 wgpu 适配器");
    block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("cull_equiv_device"),
        required_features: adapter.features() & wgpu::Features::default(),
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::default(),
        trace: wgpu::Trace::Off,
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
    }))
    .expect("请求 wgpu 设备失败")
}

fn upload_storage(
    device: &wgpu::Device,
    label: &'static str,
    notes: &[NoteInstance],
) -> wgpu::Buffer {
    use wgpu::util::DeviceExt;
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(notes),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    })
}

/// CPU 参考窗口（与 UI `collect_window_notes` + `sort_visible_notes` 同语义）。
///
/// 谓词用打包语义 `end = start + max(len, 1.0)`（与 cull shader 同式；合成数据
/// 无零长音符，与原始 `end_tick` 语义一致，见 cull.rs 零长边界注）。
fn cpu_window(
    notes: &[NoteInstance],
    tick_start: u32,
    tick_end: u32,
    key_count: usize,
) -> Vec<NoteInstance> {
    let mut out: Vec<NoteInstance> = notes
        .iter()
        .copied()
        .filter(|n| {
            let key = (n.key_color & 0xFF) as usize;
            let start = n.start_length[0].max(0.0) as u32;
            let end = start.saturating_add(n.start_length[1].max(1.0) as u32);
            key < key_count && end > tick_start && start < tick_end
        })
        .collect();
    out.sort_by(|a, b| {
        let ka = a.key_color & 0xFF;
        let kb = b.key_color & 0xFF;
        ka.cmp(&kb).then_with(|| {
            a.start_length[0]
                .max(0.0)
                .total_cmp(&b.start_length[0].max(0.0))
        })
    });
    out
}

/// 合成全集（load 序）：随机 2000 + 64 组并列（每组 3 个同 key 同 start）。
fn synthetic_full() -> Vec<NoteInstance> {
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut notes = Vec::new();
    for i in 0..2000u32 {
        let key = (next() % 128) as u8;
        let start = (next() % 9000) as f32;
        let len = (50 + next() % 1500) as f32;
        let shade = (i % 7) as f32 / 7.0;
        notes.push(NoteInstance::new(
            start,
            key,
            len,
            [0.2 + shade * 0.5, 0.9 - shade * 0.4, 0.3 + shade * 0.3, 1.0],
            0,
        ));
    }
    for g in 0..64u32 {
        let key = (g * 37 % 128) as u8;
        let start = 4000.0 + (g % 8) as f32 * 100.0;
        for v in 0..3u32 {
            notes.push(NoteInstance::new(
                start,
                key,
                200.0 + v as f32 * 300.0,
                [0.2 + v as f32 * 0.3, 0.5, 0.9 - v as f32 * 0.2, 1.0],
                0,
            ));
        }
    }
    notes
}

/// cull 提取并回读 compact（测试脚手架；生产路径见各导出 handler）。
fn cull_to_cpu(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    resident: &wgpu::Buffer,
    count: usize,
    tick_start: u32,
    tick_end: u32,
    key_count: usize,
) -> Vec<NoteInstance> {
    let mut cull = ResidentCull::new();
    cull.mark_resident_updated();
    let window = CullWindow {
        tick_start,
        tick_end,
        key_count,
    };
    let extract = cull
        .extract_count(device, queue, resident, count, window)
        .expect("cull 计数应成功");
    let (_offsets, bases, total) = prefix_counts(&extract.counts, key_count);
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("cull_equiv_fill"),
    });
    cull.extract_fill(
        device, queue, &mut enc, resident, count, window, total, &bases,
    )
    .expect("cull 填充应成功");
    let compact = cull.compact_buffer().expect("cull 紧凑缓冲应就绪").clone();
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("cull_equiv_staging"),
        size: (total * 16).max(16) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    if total > 0 {
        enc.copy_buffer_to_buffer(&compact, 0, &staging, 0, (total * 16) as u64);
    }
    queue.submit(Some(enc.finish()));
    if total == 0 {
        return Vec::new();
    }
    let need_bytes = (total * 16).max(16);
    let bytes = crate::readback_bytes_sync(device, &staging, need_bytes).expect("cull 回读应成功");
    bytemuck::cast_slice::<u8, NoteInstance>(&bytes).to_vec()
}

#[test]
fn test_cull_window_set_equals_cpu_window() {
    const TICK: u32 = 5000;
    const KEY_COUNT: usize = 128;
    let span = crate::waterfall_renderer::waterfall_viewport_span(480, 1.0);
    let tick_end = TICK.saturating_add(span);
    let notes = synthetic_full();
    let expected = cpu_window(&notes, TICK, tick_end, KEY_COUNT);

    let (device, queue) = test_device();
    let resident = upload_storage(&device, "cull_equiv_resident", &notes);
    let got = cull_to_cpu(
        &device,
        &queue,
        &resident,
        notes.len(),
        TICK,
        tick_end,
        KEY_COUNT,
    );

    assert_eq!(
        got.len(),
        expected.len(),
        "cull 窗口数量必须与 CPU 参考一致"
    );
    let got_bytes = bytemuck::cast_slice::<NoteInstance, u8>(&got);
    let expected_bytes = bytemuck::cast_slice::<NoteInstance, u8>(&expected);
    assert_eq!(
        got_bytes, expected_bytes,
        "cull 输出必须与 CPU 窗口逐字节一致（含 (key, start) 有序与并列 load 序）"
    );
}

#[test]
fn test_cull_finds_buried_long_note() {
    // 召回压力：key 60 上一覆盖长音（start=1000, end=10000）之下压 300 个死音符
    //（start ∈ (1000, 5000]，end ≤ 5000）；旧索引回溯（128 上限）到第 128 个死音符
    // 即停，漏检长音。cull 按窗口谓词提取，长音必须在 compact 中。
    const TICK: u32 = 5000;
    const KEY_COUNT: usize = 128;
    let span = crate::waterfall_renderer::waterfall_viewport_span(480, 1.0);
    let tick_end = TICK.saturating_add(span);
    let mut notes = vec![NoteInstance::new(
        1000.0,
        60,
        9000.0,
        [1.0, 0.2, 0.2, 1.0],
        0,
    )];
    for i in 0..300u32 {
        notes.push(NoteInstance::new(
            2000.0 + i as f32 * 10.0,
            60,
            5.0,
            [0.2, 0.5, 0.9, 1.0],
            0,
        ));
    }
    // 干扰：其他键随机音符（桶分区隔离性）。
    for i in 0..500u32 {
        notes.push(NoteInstance::new(
            (i * 17 % 9000) as f32,
            (i * 31 % 128) as u8,
            100.0,
            [0.5, 0.5, 0.5, 1.0],
            0,
        ));
    }

    let (device, queue) = test_device();
    let resident = upload_storage(&device, "cull_buried_resident", &notes);
    let got = cull_to_cpu(
        &device,
        &queue,
        &resident,
        notes.len(),
        TICK,
        tick_end,
        KEY_COUNT,
    );

    let found = got.iter().any(|n| {
        (n.key_color & 0xFF) == 60 && n.start_length[0] == 1000.0 && n.start_length[1] == 9000.0
    });
    assert!(found, "300 死音符下的覆盖长音必须被 cull 提取");
    // 死音符（end ≤ tick）一个都不该出现。
    let dead = got
        .iter()
        .filter(|n| {
            let s = n.start_length[0].max(0.0) as u32;
            s.saturating_add(n.start_length[1].max(1.0) as u32) <= TICK
        })
        .count();
    assert_eq!(dead, 0, "已结束音符不得进入窗口");
    // 与 CPU 参考全集一致。
    let expected = cpu_window(&notes, TICK, tick_end, KEY_COUNT);
    assert_eq!(got.len(), expected.len(), "埋藏场景窗口数量一致");
}
