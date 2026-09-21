//! Miditrail 渲染器基础烟雾测试

use super::super::*;
use futures::executor::block_on;
use wgpu::util::DeviceExt;

mod constants;
mod flat_notes;
mod instances_equivalence;
mod wgpu_smoke;

/// 渲染一帧并回读统计非黑像素（Normal/Top 视图切换测试共用）。
pub(super) fn render_and_count_non_black(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut MiditrailRenderer,
    uniform: &MiditrailUniformGpu,
    notes: &[MiditrailNoteGpu],
) -> usize {
    let (width, height) = (uniform.frame_width, uniform.frame_height);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("miditrail_switch_encoder"),
    });
    renderer.render(device, queue, &mut encoder, uniform, notes);
    let texture = renderer.output_texture().expect("渲染后应存在输出纹理");

    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded_bytes_per_row = (width * 4).next_multiple_of(align);
    let buffer_size = (padded_bytes_per_row * height) as u64;
    let staging = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("miditrail_switch_staging"),
        contents: &vec![0u8; buffer_size as usize],
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
    });
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
                bytes_per_row: Some(padded_bytes_per_row),
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
    let mut non_black = 0usize;
    for row in 0..height {
        let row_start = (row * padded_bytes_per_row) as usize;
        for col in 0..width {
            let idx = row_start + (col * 4) as usize;
            if data[idx] != 0 || data[idx + 1] != 0 || data[idx + 2] != 0 {
                non_black += 1;
            }
        }
    }
    drop(data);
    staging.unmap();
    non_black
}
