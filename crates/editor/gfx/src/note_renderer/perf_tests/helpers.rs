//! 基准共享装置 — 合成/相机/计时工具（从 perf_tests.rs 拆出）

use super::*;

pub(super) fn synth_notes(count: usize) -> Vec<NoteInstance> {
    const KEY_LOW: u32 = 21;
    const KEY_SPAN: u32 = 88;

    let mut notes = Vec::with_capacity(count);
    // 确定性伪随机（xorshift）：避免依赖 rand，且跨机器可复现
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    for _ in 0..count {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let tick = (state % TOTAL_TICKS as u64) as f32;
        let key = KEY_LOW + (state >> 32) as u32 % KEY_SPAN;
        // border_width 高 16 位 = 轨道编码（1 = 主音轨），低 16 位 = 描边像素宽
        notes.push(NoteInstance::new(
            tick,
            key as u8,
            NOTE_LEN,
            [0.2, 0.55, 1.0, 1.0],
            (1u32 << 16) | 1,
        ));
    }
    notes
}

/// 全景相机：全曲 tick + 全 128 键可见 ⇒ 所有音符通过 cull（piano roll 缩小到底）
pub(super) fn panorama_camera() -> CameraUniform {
    CameraUniform {
        scroll: [0.0, 0.0],
        zoom: [
            (BENCH_W as f32 - KEYBOARD_W) / TOTAL_TICKS,
            (BENCH_H as f32 - RULER_H) / KEY_COUNT as f32,
        ],
        viewport_size: [BENCH_W as f32, BENCH_H as f32],
        canvas_offset: [0.0, 0.0],
        canvas_size: [BENCH_W as f32, BENCH_H as f32],
        keyboard_width: KEYBOARD_W,
        ruler_height: RULER_H,
        max_key_index: (KEY_COUNT - 1) as f32,
        _padding: [0.0; 3],
    }
}

/// 局部放大相机：约 0.06% tick × 52 键可见 ⇒ cull 剔除绝大多数音符
pub(super) fn zoomed_camera() -> CameraUniform {
    CameraUniform {
        zoom: [2.0, 20.0],
        ..panorama_camera()
    }
}

/// 提交单个 encoder 并**有界等待** GPU 完成，返回墙钟耗时。
pub(super) fn submit_and_wait(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    encoder: wgpu::CommandEncoder,
) -> Duration {
    let start = Instant::now();
    let _ = queue.submit(Some(encoder.finish()));
    // 有界等待：禁止 timeout: None（无限阻塞会让基准在驱动异常时挂死）
    let _ = device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(Duration::from_secs(60)),
    });
    start.elapsed()
}

/// 对 `encode` 重复 `iters` 轮，返回中位数耗时（**丢弃前 2 轮预热**：
/// 首轮含驱动惰性分配与时钟爬升，第二轮仍可能受电源状态影响）。
pub(super) fn median_time(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    iters: usize,
    mut encode: impl FnMut(&mut wgpu::CommandEncoder),
) -> Duration {
    const WARMUP_ROUNDS: usize = 2;
    let mut samples: Vec<Duration> = Vec::with_capacity(iters);
    for round in 0..iters {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("note_perf_bench"),
        });
        encode(&mut encoder);
        let elapsed = submit_and_wait(device, queue, encoder);
        if round >= WARMUP_ROUNDS {
            samples.push(elapsed);
        }
    }
    samples.sort_unstable();
    samples
        .get(samples.len() / 2)
        .copied()
        .unwrap_or(Duration::ZERO)
}

pub(super) fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// 有界回读 staging（复刻 depth_tests 的非阻塞轮询模式）。
pub(super) fn read_u32(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    buffer: &wgpu::Buffer,
    offset: u64,
) -> u32 {
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("note_perf_readback"),
        size: 4,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("note_perf_readback_encoder"),
    });
    encoder.copy_buffer_to_buffer(buffer, offset, &staging, 0, 4);
    queue.submit(Some(encoder.finish()));

    let slice = staging.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let _ = device.poll(wgpu::PollType::Poll);
        match rx.try_recv() {
            Ok(Ok(())) => break,
            Ok(Err(e)) => panic!("回读 map 失败: {e:?}"),
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                assert!(Instant::now() < deadline, "回读 30s 未就绪");
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => panic!("回读回调丢失"),
        }
    }
    let value = {
        let data = slice.get_mapped_range();
        u32::from_ne_bytes([data[0], data[1], data[2], data[3]])
    };
    staging.unmap();
    value
}
