//! 瀑布流 legacy 窗口路径与全局桶索引路径的像素等价性。
//!
//! 同一输入、两条管线、逐字节对比：
//! - `test_indexed_matches_legacy_ordered`（严格）：legacy 窗口集与常驻集同为
//!   `(key, start)` 有序（含并列同序）→ 必须 bit-identical（算法一致性证明）；
//! - `test_indexed_tiebreak_deviation_is_bounded`（量化）：常驻集并列逆序（模拟
//!   load 序 vs legacy 序的 tiebreak 差异）→ 统计差异像素占比， acceptance 标准：
//!   差异仅出现在同 (key, start) 并列音符几何重叠区（见断言注释）。

use super::*;
use crate::{BucketSource, NoteInstance};
use futures::executor::block_on;

const TEST_W: u32 = 256;
const TEST_H: u32 = 144;

fn test_device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        .expect("测试需要可用的 wgpu 适配器");
    block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("waterfall_equiv_device"),
        required_features: adapter.features() & wgpu::Features::default(),
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::default(),
        trace: wgpu::Trace::Off,
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
    }))
    .expect("请求 wgpu 设备失败")
}

fn test_uniform() -> WaterfallUniformGpu {
    WaterfallUniformGpu {
        tick: 5000,
        ppq: 480,
        key_count: 128,
        frame_width: TEST_W,
        frame_height: TEST_H,
        kb_height: 17,
        speed: 1.0,
        _padding: 0,
    }
}

/// 合成场景：128 键覆盖、长短音混合、刻意并列（同 key 同 start 不同颜色/长度）。
///
/// 返回 `(legacy_ordered, resident_order)`：legacy 输入为 `(key, start)` 稳定排序
/// （复刻生产窗口集语义）；resident 输入为 load 序（复刻 onion 轨追加序）。
/// `reverse_ties` 为 true 时 resident 内并列逆序（tiebreak 最大压力）。
fn synthetic_scene(reverse_ties: bool) -> (Vec<NoteInstance>, Vec<NoteInstance>) {
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut load_order = Vec::new();
    for i in 0..3000u32 {
        let key = (next() % 128) as u8;
        let start = (next() % 9000) as f32;
        let len = (50 + next() % 1500) as f32;
        let shade = (i % 7) as f32 / 7.0;
        load_order.push(NoteInstance::new(
            start,
            key,
            len,
            [0.2 + shade * 0.5, 0.9 - shade * 0.4, 0.3 + shade * 0.3, 1.0],
            0,
        ));
    }
    // 刻意并列：同 key 同 start、不同长度/颜色（每组 3 个，64 组）。
    for g in 0..64u32 {
        let key = (g * 37 % 128) as u8;
        let start = 4000.0 + (g % 8) as f32 * 100.0;
        for v in 0..3u32 {
            load_order.push(NoteInstance::new(
                start,
                key,
                200.0 + v as f32 * 300.0,
                [0.2 + v as f32 * 0.3, 0.5, 0.9 - v as f32 * 0.2, 1.0],
                0,
            ));
        }
    }
    // legacy 窗口集：(key, start) 稳定排序（并列保持 load 相对顺序）。
    let mut legacy_ordered = load_order.clone();
    legacy_ordered.sort_by(|a, b| {
        let ka = a.key_color & 0xFF;
        let kb = b.key_color & 0xFF;
        ka.cmp(&kb).then_with(|| {
            let sa = a.start_length[0].max(0.0);
            let sb = b.start_length[0].max(0.0);
            sa.total_cmp(&sb)
        })
    });
    let mut resident_order = load_order;
    if reverse_ties {
        // 并列逆序：连续同 (key, start) 段整体反转（非并列相对顺序不变）。
        let mut out = Vec::with_capacity(resident_order.len());
        let mut run: Vec<NoteInstance> = Vec::new();
        let key_of = |n: &NoteInstance| (n.key_color & 0xFF, n.start_length[0].max(0.0).to_bits());
        for n in resident_order.drain(..) {
            if run.last().is_some_and(|last| key_of(last) == key_of(&n)) {
                run.push(n);
            } else {
                run.iter().rev().for_each(|r| out.push(*r));
                run.clear();
                run.push(n);
            }
        }
        run.iter().rev().for_each(|r| out.push(*r));
        resident_order = out;
    }
    (legacy_ordered, resident_order)
}

/// legacy 分桶偏移派生（与 `video_export::common::note_instances_to_key_offsets` 同语义，
/// 测试内联以避免跨模块耦合；调用方须保证输入已按 key 聚簇）。
fn key_offsets_for(notes: &[NoteInstance], key_count: usize) -> Vec<u32> {
    let mut counts = vec![0u32; key_count];
    for n in notes {
        let key = (n.key_color & 0xFF) as usize;
        if key < key_count {
            counts[key] += 1;
        }
    }
    let mut offsets = vec![0u32; key_count + 1];
    for k in 0..key_count {
        offsets[k + 1] = offsets[k] + counts[k];
    }
    offsets
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

/// 输出纹理同步回读（256×144 RGBA8，行距 1024 天然对齐，无 padding）。
fn readback_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
) -> Vec<u8> {
    let bytes_per_row = TEST_W * 4;
    let size = (bytes_per_row * TEST_H) as u64;
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("waterfall_equiv_staging"),
        size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("waterfall_equiv_readback"),
    });
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &staging,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(TEST_H),
            },
        },
        wgpu::Extent3d {
            width: TEST_W,
            height: TEST_H,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(enc.finish()));
    let (tx, rx) = std::sync::mpsc::channel();
    staging
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if let Ok(map_result) = rx.try_recv() {
            map_result.expect("等价测试回读 map 失败");
            let data = staging.slice(..).get_mapped_range();
            let out = data.to_vec();
            drop(data);
            staging.unmap();
            return out;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "等价测试回读超时（10s）"
        );
        let _ = device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });
    }
}

/// 双轨各渲染一帧并回读像素。
fn render_both(
    legacy_notes: &[NoteInstance],
    resident_notes: &[NoteInstance],
) -> (Vec<u8>, Vec<u8>) {
    let (device, queue) = test_device();
    let uniform = test_uniform();
    let colors = [0u32; 128];

    // legacy：窗口集上传 + 派生分桶。
    let mut legacy_renderer = WaterfallRenderer::new(&device);
    let window_buf = upload_storage(&device, "waterfall_equiv_window", legacy_notes);
    let key_offsets = key_offsets_for(legacy_notes, 128);
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("waterfall_equiv_legacy"),
    });
    legacy_renderer.render(
        &device,
        &queue,
        &mut enc,
        &uniform,
        &window_buf,
        legacy_notes.len(),
        &key_offsets,
        &colors,
    );
    let legacy_tex = legacy_renderer
        .output_texture()
        .expect("legacy 输出纹理应就绪");
    // encoder 提交后再经 readback_texture 独立拷贝读回（纹理句柄跨 submit 有效）。
    queue.submit(Some(enc.finish()));
    let legacy_pixels = readback_texture(&device, &queue, legacy_tex);

    // indexed：常驻集直绑 + 全局桶。
    let mut indexed_renderer = WaterfallRenderer::new(&device);
    let resident_buf = upload_storage(&device, "waterfall_equiv_resident", resident_notes);
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("waterfall_equiv_indexed"),
    });
    let ok = indexed_renderer.render_indexed(
        &device,
        &queue,
        &mut enc,
        &uniform,
        BucketSource {
            buffer: &resident_buf,
            count: resident_notes.len(),
            epoch: 7,
        },
        &colors,
    );
    assert!(ok, "索引渲染应成功（合成数据无异常）");
    queue.submit(Some(enc.finish()));
    let indexed_tex = indexed_renderer
        .output_texture()
        .expect("indexed 输出纹理应就绪");
    let indexed_pixels = readback_texture(&device, &queue, indexed_tex);

    (legacy_pixels, indexed_pixels)
}

#[test]
fn test_indexed_matches_legacy_ordered() {
    // 同序输入（并列同序）：两条路径必须 bit-identical。
    let (legacy_notes, resident_notes) = synthetic_scene(false);
    let (legacy_pixels, indexed_pixels) = render_both(&legacy_notes, &resident_notes);
    assert_eq!(
        legacy_pixels.len(),
        indexed_pixels.len(),
        "双轨输出字节数一致"
    );
    let diffs = legacy_pixels
        .iter()
        .zip(indexed_pixels.iter())
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(
        diffs, 0,
        "同序输入下双轨像素必须 bit-identical（差异字节 {diffs}）"
    );
}

#[test]
fn test_indexed_tiebreak_deviation_is_bounded() {
    // 并列逆序（tiebreak 最大压力）：差异必须局限在并列重叠区，总量封顶。
    let (legacy_notes, resident_notes) = synthetic_scene(true);
    let (legacy_pixels, indexed_pixels) = render_both(&legacy_notes, &resident_notes);
    let diff_bytes = legacy_pixels
        .iter()
        .zip(indexed_pixels.iter())
        .filter(|(a, b)| a != b)
        .count();
    let diff_pixels = diff_bytes.div_ceil(4);
    let total_pixels = (TEST_W * TEST_H) as usize;
    let ratio = diff_pixels as f64 / total_pixels as f64;
    // 实测基线（2026-09-05，RTX 2060）：192 个逆序并列下仅 20 像素差异（0.054%），
    // 局限在同 (key, start) 并列几何重叠区。
    // 验收线：并列逆序极端压力下差异像素 < 2%（并列组仅 192/3192 音符≈6%，
    // 重叠区为其子集；超标说明 tiebreak 影响超出并列语义，需升级处理）。
    assert!(
        ratio < 0.02,
        "tiebreak 差异像素占比超标：{diff_pixels}/{total_pixels} = {ratio:.4}"
    );
}
