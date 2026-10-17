//! GpuNoteBuffer 真机 GPU 单测：CPU 镜像删除后增量仍以 GPU 为唯一权威
//!
//! 2026-10-17 单一权威源整改验收（`ops/upload.rs` / `ops/update.rs` /
//! `ops/incremental.rs`）：
//! - `remove_note` 的 swap-remove 必须在 GPU 内完成。旧实现从 `self.instances`
//!   取末尾实例——镜像删除后该路径必然越界 panic，因此本文件是「增量路径真的
//!   不再依赖 CPU 镜像」的可执行证据，而不是靠注释自证。
//! - `upload_all` / `update_notes` / `add_note` / `insert_at` / `remove_at` /
//!   `remove_note` / 流式上传组合执行后，GPU 缓冲内容与 CPU 参考模型逐字节一致。

use crate::NoteInstance;
use crate::gpu_note_buffer::GpuNoteBuffer;

fn test_device() -> (wgpu::Device, wgpu::Queue) {
    crate::test_gpu::shared_device()
}

/// 确定性实例：tick/key 唯一 → 便于断言「谁被搬到了哪里」
fn inst(tick: f32, key: u8) -> NoteInstance {
    NoteInstance::new(tick, key, 10.0, [1.0, 0.0, 0.0, 1.0], 1)
}

/// GPU 实例缓冲前 `count` 个实例 → CPU（同步回读）
fn read_instances(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    src: &wgpu::Buffer,
    count: usize,
) -> Vec<NoteInstance> {
    let bytes = count * std::mem::size_of::<NoteInstance>();
    if bytes == 0 {
        return Vec::new();
    }
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("gpu_note_buffer_test_staging"),
        size: bytes as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("gpu_note_buffer_test_readback"),
    });
    enc.copy_buffer_to_buffer(src, 0, &staging, 0, bytes as u64);
    queue.submit(Some(enc.finish()));
    let raw = crate::global_bucket::readback_bytes_sync(device, &staging, bytes)
        .expect("实例缓冲回读应成功");
    bytemuck::cast_slice::<u8, NoteInstance>(&raw).to_vec()
}

/// 逐字节比较（`NoteInstance` 无 `PartialEq`；此处要的正是位模式一致）
fn assert_instances(actual: &[NoteInstance], expected: &[NoteInstance], ctx: &str) {
    assert_eq!(actual.len(), expected.len(), "{ctx}: 实例数不一致");
    let a: &[u8] = bytemuck::cast_slice(actual);
    let b: &[u8] = bytemuck::cast_slice(expected);
    assert_eq!(a, b, "{ctx}: 实例字节不一致");
}

/// swap-remove 在 GPU 内搬移末位实例（无 CPU 镜像可索引）
#[test]
fn swap_remove_moves_last_instance_on_gpu_without_cpu_mirror() {
    let (device, queue) = test_device();
    let mut buf = GpuNoteBuffer::new(&device, &queue);
    let a = inst(1.0, 10);
    let b = inst(2.0, 11);
    let c = inst(3.0, 12);
    buf.upload_all(&[a, b, c]);

    // 删首位（非末位）→ 末位 C 搬到 index 0
    buf.remove_note(0);
    assert_eq!(buf.len(), 2, "删除后计数");
    let got = read_instances(&device, &queue, buf.buffer(), buf.len());
    assert_instances(&got, &[c, b], "swap-remove 首位");

    // 删末位 → 无需搬移
    buf.remove_note(1);
    assert_eq!(buf.len(), 1, "删除末位后计数");
    let got = read_instances(&device, &queue, buf.buffer(), buf.len());
    assert_instances(&got, &[c], "移除末位");

    // 越界删除为无害 no-op
    buf.remove_note(9);
    assert_eq!(buf.len(), 1, "越界删除不得改变计数");
}

/// 全增量 API 序列 = CPU 参考模型（唯一权威在 GPU）
#[test]
fn incremental_ops_are_gpu_authoritative() {
    let (device, queue) = test_device();
    let mut buf = GpuNoteBuffer::new(&device, &queue);

    let n0 = inst(0.0, 60);
    let n1 = inst(10.0, 61);
    let n2 = inst(20.0, 62);
    let n3 = inst(30.0, 63);
    let n1b = inst(11.0, 71);
    let x = inst(1.0, 70);
    let y = inst(2.0, 72);

    let mut expect: Vec<NoteInstance> = vec![n0, n1, n2];
    buf.upload_all(&expect);

    // 1) 原位批量更新
    buf.update_notes(1, &[n1b]);
    expect[1] = n1b;

    // 2) 末尾追加
    let appended_at = buf.add_note(&n3);
    assert_eq!(appended_at, 3, "add_note 应返回追加位置");
    expect.push(n3);

    // 3) 段内保序插入
    buf.insert_at(1, &[x, y]);
    expect.splice(1..1, [x, y]);

    // 4) 保序区间删除 [2, 4)
    buf.remove_at(2, 2);
    expect.drain(2..4);

    // 5) swap-remove 首位（与 Vec::swap_remove 同语义）
    buf.remove_note(0);
    expect.swap_remove(0);

    assert_eq!(buf.len(), expect.len(), "终态计数");
    let got = read_instances(&device, &queue, buf.buffer(), buf.len());
    assert_instances(&got, &expect, "增量序列终态");
}

/// 流式上传后继续增量（旧实现在镜像为空时 `update_notes` 语义可疑）
#[test]
fn streaming_upload_then_incremental_stays_gpu_only() {
    let (device, queue) = test_device();
    let mut buf = GpuNoteBuffer::new(&device, &queue);
    let a = inst(0.0, 60);
    let b = inst(5.0, 62);

    buf.begin_streaming_upload();
    buf.streaming_append(&[a]);
    buf.streaming_append(&[b]);
    buf.finish_streaming_upload();
    assert_eq!(buf.len(), 2, "流式上传计数");

    // 流式上传后原位更新：必须只改 GPU
    let a2 = inst(1.0, 61);
    buf.update_notes(0, &[a2]);
    let got = read_instances(&device, &queue, buf.buffer(), buf.len());
    assert_instances(&got, &[a2, b], "流式上传后原位更新");

    // clear 只重置计数；随后重新全量上传仍正确（旧语义靠镜像清空，现已无镜像）
    buf.clear();
    assert_eq!(buf.len(), 0, "clear 后计数");
    buf.upload_all(&[b, a]);
    let got = read_instances(&device, &queue, buf.buffer(), buf.len());
    assert_instances(&got, &[b, a], "clear 后重新全量上传");
}
