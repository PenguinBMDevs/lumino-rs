//! GPU-Driven 与 legacy 路径等价性：同一输入、两条管线、像素级对比。
//!
//! 背景：真机 dense 帧发现键盘区亮度差异（f0/f100 一致，f300 有 wash 缺失），
//! 本模块把差分收敛为可复现的单元测试：CPU 融合扫描逐位对比＋GPU 像素对比。

use super::super::instances::{
    build_aura_instances, compute_active_and_aura_for_compact, compute_active_keys,
    emit_aura_instances, update_key_positions,
};
use super::super::*;
use super::paint_order_window;
use crate::{CullWindow, NoteInstance};
use futures::executor::block_on;
use wgpu::util::DeviceExt;

mod compact;
mod coplanar;
mod fused_scan;
mod pixels;
mod preview;

/// 高密度合成场景：128 键全覆盖、起始交错（active/未开始/已结束混合）、
///
/// 同键叠音（稳定性）与黑白键重叠（覆盖序），复刻真机 dense 帧的特征。
pub(crate) fn dense_scene() -> Vec<NoteInstance> {
    let mut notes = Vec::new();
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    // tick=5000 处约 1/3 active、1/3 未开始、1/3 已结束（collect 语义只留 end>tick，
    // 此处故意混入已结束音符验证 legacy/Driven 过滤一致性——Driven 由 shader 剔除）。
    for i in 0..6000u32 {
        let key = (next() % 128) as u8;
        let start = (next() % 9000) as f32;
        let len = (50 + next() % 1500) as f32;
        let shade = (i % 5) as f32 * 0.2;
        notes.push(NoteInstance::new(
            start,
            key,
            len,
            [0.2 + shade, 0.9 - shade * 0.5, 0.3, 1.0],
            0,
        ));
    }
    notes
}

pub(crate) fn test_uniform() -> MiditrailUniformGpu {
    MiditrailUniformGpu {
        tick: 5000,
        ppq: 480,
        key_count: 128,
        frame_width: 640,
        frame_height: 360,
        kb_height: 43,
        _reserved: 0,
        speed: 1.0,
        param1: 0.0,
        param2: 0.0,
        fps: 60.0,
        z_far_distance: 7.5,
        view_mode: MiditrailViewMode::Normal,
        ticks_per_second: 960.0,
        _padding1: 0,
    }
}

pub(crate) fn test_device() -> (wgpu::Instance, wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        .expect("测试需要可用的 wgpu 适配器");
    let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("miditrail_driven_equiv_device"),
        required_features: adapter.features() & wgpu::Features::default(),
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::default(),
        trace: wgpu::Trace::Off,
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
    }))
    .expect("请求 wgpu 设备失败");
    (instance, device, queue)
}

/// 回读一帧 RGBA（去 row padding）。
pub(crate) fn readback_pixels(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    encoder: wgpu::CommandEncoder,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded = (width * 4).next_multiple_of(align);
    let staging = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("miditrail_driven_equiv_staging"),
        contents: &vec![0u8; (padded * height) as usize],
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
    });
    let mut encoder = encoder;
    encoder.copy_texture_to_buffer(
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
                bytes_per_row: Some(padded),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(std::iter::once(encoder.finish()));
    let slice = staging.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        tx.send(r).expect("map_async 回调发送失败");
    });
    let _ = device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    });
    rx.recv()
        .expect("map_async 回调未收到")
        .expect("map_async 失败");
    let data = slice.get_mapped_range();
    let mut out = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height {
        let s = (row * padded) as usize;
        out.extend_from_slice(&data[s..s + (width * 4) as usize]);
    }
    drop(data);
    staging.unmap();
    out
}
