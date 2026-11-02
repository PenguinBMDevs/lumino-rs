//! Step 5b 验收：脏矩形局部上传（上传计划 + GPU 读回像素一致性）。
//!
//! 局部上传的正确性只有一条判据：**写进去的是 B 的脏区，没写进去的仍是 A 的
//! 旧像素**。因此帧 B 在脏区之外故意使用另一种颜色——若实现退化成全量上传
//!（或错误清空纹理），下面「区域外 = A」的断言会立刻打到 B 的新颜色。

use super::{
    CpuOnionFrame, CpuOnionRenderer, CpuOnionUploadRect,
    tests_support::{TARGET_FORMAT, pixel, render_and_readback, transient_device},
    upload::{UploadPlan, UploadState},
};

/// 测试纹理边长（4x4 足够覆盖「区域内 / 区域外 / 行距 / 末行」四件事）。
const SIZE: u32 = 4;

/// 不透明红。
const RED: [u8; 4] = [255, 0, 0, 255];
/// 不透明绿。
const GREEN: [u8; 4] = [0, 255, 0, 255];
/// 不透明品红（帧 B 在脏区之外的「对抗色」：出现即说明发生了全量上传）。
const MAGENTA: [u8; 4] = [255, 0, 255, 255];
/// 不透明蓝。
const BLUE: [u8; 4] = [0, 0, 255, 255];
/// 不透明青。
const CYAN: [u8; 4] = [0, 255, 255, 255];

/// `SIZE × SIZE` 纯色帧（无脏区 = 整帧语义）。
fn solid(rgba: [u8; 4], version: u64) -> CpuOnionFrame {
    let len = CpuOnionFrame::expected_len(SIZE, SIZE).expect("4x4 像素字节数不得溢出");
    let mut pixels = vec![0u8; len];
    for chunk in pixels.as_chunks_mut::<4>().0 {
        chunk.copy_from_slice(&rgba);
    }
    CpuOnionFrame::new(0, 0, SIZE, SIZE, pixels, version)
}

/// 同尺寸帧：`rect` 内 = `patch`，其余 = `fill`；脏区标注为 `rect`。
fn solid_with_rect(
    fill: [u8; 4],
    rect: CpuOnionUploadRect,
    patch: [u8; 4],
    version: u64,
) -> CpuOnionFrame {
    let len = CpuOnionFrame::expected_len(SIZE, SIZE).expect("4x4 像素字节数不得溢出");
    let mut pixels = vec![0u8; len];
    for (index, chunk) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let x = index as u32 % SIZE;
        let y = index as u32 / SIZE;
        let inside = x >= rect.x
            && x < rect.x.saturating_add(rect.width)
            && y >= rect.y
            && y < rect.y.saturating_add(rect.height);
        chunk.copy_from_slice(if inside { &patch } else { &fill });
    }
    CpuOnionFrame::new(0, 0, SIZE, SIZE, pixels, version).with_dirty_rect(Some(rect))
}

/// 整帧读回是否全部等于 `rgba`。
fn assert_frame_all(bytes: &[u8], rgba: [u8; 4], ctx: &str) {
    for y in 0..SIZE {
        for x in 0..SIZE {
            assert_eq!(
                pixel(bytes, SIZE, x, y),
                rgba,
                "{ctx}：({x}, {y}) 全量上传后整帧必须等于新帧"
            );
        }
    }
}

/// 局部上传核心验收：脏区内 = B，脏区外 = A（旧像素未被覆盖 / 未被清空）。
#[test]
fn cpu_onion_partial_upload_preserves_pixels_outside_dirty_rect() {
    let (_instance, device, queue) = transient_device();
    let mut renderer = CpuOnionRenderer::new(&device, &queue, TARGET_FORMAT);

    // 帧 A：整帧不透明红（首帧 → 重建纹理 + 全量上传）
    let a = solid(RED, 1);
    renderer.prepare(&queue, &a);
    assert_eq!(renderer.uploaded_version(), Some(1));

    // 帧 B：同尺寸、version + 1；只有 (1,1,2,1) 变绿，其余是「对抗色」品红。
    // 若实现退化成全量上传，区域外会读成品红 → 测试失败。
    let rect = CpuOnionUploadRect::new(1, 1, 2, 1);
    let b = solid_with_rect(MAGENTA, rect, GREEN, 2);
    renderer.prepare(&queue, &b);
    assert_eq!(
        renderer.uploaded_version(),
        Some(2),
        "局部上传成功后必须记录新版本"
    );

    let bytes = render_and_readback(&device, &queue, &mut renderer, &b);
    // 脏区内 = B 的像素
    for x in [1, 2] {
        assert_eq!(
            pixel(&bytes, SIZE, x, 1),
            GREEN,
            "脏区内的像素必须来自帧 B：({x}, 1)"
        );
    }
    // 脏区外 = A 的旧像素（未被覆盖、也未被清空）
    for y in 0..SIZE {
        for x in 0..SIZE {
            let inside_x = x >= rect.x && x < rect.x + rect.width;
            let inside_y = y >= rect.y && y < rect.y + rect.height;
            if inside_x && inside_y {
                continue;
            }
            assert_eq!(
                pixel(&bytes, SIZE, x, y),
                RED,
                "脏区外必须保留上一版像素（不得被覆盖/清空）：({x}, {y})"
            );
        }
    }
}

/// 末行脏区：验证源切片行距（`frame.width * 4`）与末行字节数计算不越界、不串行。
#[test]
fn cpu_onion_partial_upload_on_last_row_keeps_other_rows() {
    let (_instance, device, queue) = transient_device();
    let mut renderer = CpuOnionRenderer::new(&device, &queue, TARGET_FORMAT);

    renderer.prepare(&queue, &solid(RED, 1));
    let rect = CpuOnionUploadRect::new(0, SIZE - 1, SIZE, 1);
    let b = solid_with_rect(MAGENTA, rect, GREEN, 2);
    renderer.prepare(&queue, &b);
    assert_eq!(renderer.uploaded_version(), Some(2));

    let bytes = render_and_readback(&device, &queue, &mut renderer, &b);
    for x in 0..SIZE {
        assert_eq!(
            pixel(&bytes, SIZE, x, SIZE - 1),
            GREEN,
            "末行脏区内的像素必须更新：({x}, {})",
            SIZE - 1
        );
    }
    for y in 0..SIZE - 1 {
        for x in 0..SIZE {
            assert_eq!(
                pixel(&bytes, SIZE, x, y),
                RED,
                "末行以外的行必须保留旧像素：({x}, {y})"
            );
        }
    }
}

/// 同版本再 prepare：`Reuse`，不重新上传（用「同版本但内容不同」的帧反证）。
#[test]
fn cpu_onion_same_version_does_not_upload_again() {
    let (_instance, device, queue) = transient_device();
    let mut renderer = CpuOnionRenderer::new(&device, &queue, TARGET_FORMAT);

    renderer.prepare(&queue, &solid(RED, 1));
    let rect = CpuOnionUploadRect::new(1, 1, 2, 1);
    let b = solid_with_rect(MAGENTA, rect, GREEN, 2);
    renderer.prepare(&queue, &b);
    assert_eq!(renderer.uploaded_version(), Some(2));
    let before = render_and_readback(&device, &queue, &mut renderer, &b);

    // 同 version 2、同尺寸，整帧改成蓝色（脏区隐式无意义）→ 必须 Reuse 不上传
    let same_version = solid(BLUE, 2);
    renderer.prepare(&queue, &same_version);
    assert_eq!(
        renderer.uploaded_version(),
        Some(2),
        "同版本不得推进版本（即不得重新上传）"
    );
    let after = render_and_readback(&device, &queue, &mut renderer, &same_version);
    assert_eq!(
        after, before,
        "同版本再次 prepare 必须完全复用驻留纹理（像素逐字节不变）"
    );
}

/// 版本变化但无脏区 / 脏区越界 → 全量上传，整帧等于新帧。
#[test]
fn cpu_onion_full_upload_when_dirty_missing_or_invalid() {
    let (_instance, device, queue) = transient_device();
    let mut renderer = CpuOnionRenderer::new(&device, &queue, TARGET_FORMAT);
    renderer.prepare(&queue, &solid(RED, 1));

    // 无脏区 → 全量
    let no_dirty = solid(BLUE, 2);
    renderer.prepare(&queue, &no_dirty);
    assert_eq!(renderer.uploaded_version(), Some(2));
    let bytes = render_and_readback(&device, &queue, &mut renderer, &no_dirty);
    assert_frame_all(&bytes, BLUE, "无脏区");

    // 脏区越界（3,3,4,4 只覆盖右下角 1 像素，越界部分不可表达）→ 保守全量
    let oob = solid(CYAN, 3).with_dirty_rect(Some(CpuOnionUploadRect::new(3, 3, 4, 4)));
    renderer.prepare(&queue, &oob);
    assert_eq!(renderer.uploaded_version(), Some(3));
    let bytes = render_and_readback(&device, &queue, &mut renderer, &oob);
    assert_frame_all(&bytes, CYAN, "脏区越界");
}

/// 上传状态自相矛盾（内部记录与资源不一致）时，局部上传被拒后不得画错帧。
///
/// 直接用 `UploadState` 演示判定：脏区非法一律回退 `Upload`（渲染器侧失败会
/// `reset` 并在下一帧重建，见 `renderer::prepare`）。
#[test]
fn cpu_onion_partial_plan_rejects_invalid_rect() {
    let mut state = UploadState::default();
    let frame = solid(RED, 1);
    state.record(&frame);

    // 合法脏区 → 局部；越界 → 全量
    let valid = solid_with_rect(MAGENTA, CpuOnionUploadRect::new(0, 0, 2, 2), GREEN, 2);
    assert_eq!(
        state.plan(&valid),
        UploadPlan::PartialUpload(CpuOnionUploadRect::new(0, 0, 2, 2))
    );
    let invalid =
        solid(CYAN, 3).with_dirty_rect(Some(CpuOnionUploadRect::new(0, 0, SIZE, SIZE + 1)));
    assert_eq!(state.plan(&invalid), UploadPlan::Upload);
}

/// 尺寸变化必须重建纹理并全量上传（脏区被忽略）。
#[test]
fn cpu_onion_size_change_recreates_and_ignores_dirty() {
    let (_instance, device, queue) = transient_device();
    let mut renderer = CpuOnionRenderer::new(&device, &queue, TARGET_FORMAT);
    renderer.prepare(&queue, &solid(RED, 1));

    let len = CpuOnionFrame::expected_len(2, 2).expect("2x2 像素字节数不得溢出");
    let mut pixels = vec![0u8; len];
    for chunk in pixels.as_chunks_mut::<4>().0 {
        chunk.copy_from_slice(&GREEN);
    }
    let resized = CpuOnionFrame::new(0, 0, 2, 2, pixels, 2)
        .with_dirty_rect(Some(CpuOnionUploadRect::new(0, 0, 1, 1)));
    renderer.prepare(&queue, &resized);
    assert_eq!(renderer.uploaded_version(), Some(2));

    let bytes = render_and_readback(&device, &queue, &mut renderer, &resized);
    for y in 0..2 {
        for x in 0..2 {
            assert_eq!(
                pixel(&bytes, 2, x, y),
                GREEN,
                "新尺寸纹理必须整帧铺满（脏区被忽略）：({x}, {y})"
            );
        }
    }
}

/// 像素帧不是紧凑子图：脏区源数据来自整帧像素（行距 = 帧宽），不是区域切片。
#[test]
fn cpu_onion_partial_upload_reads_from_full_frame_stride() {
    let (_instance, device, queue) = transient_device();
    let mut renderer = CpuOnionRenderer::new(&device, &queue, TARGET_FORMAT);

    // 帧 A：每行不同色（红/绿/蓝/品红），便于发现「行距用错」
    let mut pixels = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    let row_colors = [RED, GREEN, BLUE, MAGENTA];
    for row in &row_colors {
        for _ in 0..SIZE {
            pixels.extend_from_slice(row);
        }
    }
    renderer.prepare(&queue, &CpuOnionFrame::new(0, 0, SIZE, SIZE, pixels, 1));

    // 帧 B：只把第 1 行第 0 列改成青色（脏区 1x1），其余行沿用 A 的内容
    let rect = CpuOnionUploadRect::new(0, 1, 1, 1);
    let mut pixels = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for (index, row) in row_colors.iter().enumerate() {
        for column in 0..SIZE {
            if index == 1 && column == 0 {
                pixels.extend_from_slice(&CYAN);
            } else {
                pixels.extend_from_slice(row);
            }
        }
    }
    let b = CpuOnionFrame::new(0, 0, SIZE, SIZE, pixels, 2).with_dirty_rect(Some(rect));
    renderer.prepare(&queue, &b);
    assert_eq!(renderer.uploaded_version(), Some(2));

    let bytes = render_and_readback(&device, &queue, &mut renderer, &b);
    assert_eq!(pixel(&bytes, SIZE, 0, 1), CYAN, "脏区像素必须命中第 1 行");
    assert_eq!(pixel(&bytes, SIZE, 1, 1), GREEN, "同一行其它像素不变");
    for (y, row) in row_colors.iter().enumerate() {
        if y == 1 {
            continue;
        }
        for x in 0..SIZE {
            assert_eq!(
                pixel(&bytes, SIZE, x, y as u32),
                *row,
                "其它行必须保留 A 的行内容（源行距 = 帧宽）：({x}, {y})"
            );
        }
    }
}
