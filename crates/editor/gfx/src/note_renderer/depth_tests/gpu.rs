//! 深度像素证据 — GPU 测试（从 depth_tests.rs 拆出）

use super::gpu_common::*;
use super::*;

/// 核心回归测试：同一场景、同一源数据，仅改变可见索引的输出顺序，
/// 连续 64 帧像素必须逐位一致；并锁定确定性的叠压赢家与预览层级。
#[test]
fn test_overlap_pixels_are_draw_order_independent() {
    let _serial = lock_depth_gpu();
    let (device, queue) = crate::pipeline::test_device();
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;

    // 适配器能力门控：软渲染器压平亚 2^-24 深度时，本断言不可判定（见探针说明）
    if !adapter_resolves_region_depth(&device, &queue, format) {
        eprintln!(
            "跳过 test_overlap_pixels_are_draw_order_independent：适配器无法分辨区域深度（软渲染）"
        );
        return;
    }

    let onion_notes = onion_scene();
    let preview_notes = preview_scene();

    let mut onion = NoteRenderer::new_onion_skin(&device, &queue, format);
    // track_enc = 1 为主音轨（主轨蓝 + 主音轨区深度）
    onion.set_view_state(&queue, 1, &[]);
    onion.upload_instances(&onion_notes, &device, &queue);

    let mut note = NoteRenderer::new(&device, &queue, format);
    note.upload_instances(&preview_notes, &device, &queue);

    write_camera(&onion, &queue);
    write_camera(&note, &queue);

    let color_texture = make_color_texture(&device, format);
    let depth_texture = make_depth_texture(&device);
    let color_view = color_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

    let mut baseline: Option<Vec<u8>> = None;
    for frame in 0..ORDER_FRAMES {
        write_visible_order(&onion, &queue, &permuted_order(onion_notes.len(), frame));
        write_visible_order(
            &note,
            &queue,
            &permuted_order(preview_notes.len(), frame.wrapping_mul(7)),
        );

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("note_depth_order_encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("note_depth_order_pass"),
                color_attachments: &[Some(color_attachment(&color_view))],
                depth_stencil_attachment: Some(depth_attachment(&depth_view)),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            // 与生产同序：洋葱皮（含主音轨）先画，预览音符后画
            onion.draw(&mut pass, true, None);
            note.draw(&mut pass, true, None);
        }
        queue.submit(Some(encoder.finish()));

        let pixels = readback_pixels(&device, &queue, &color_texture);
        match baseline.as_ref() {
            None => {
                // 叠压赢家锁定：索引最小（depth 最小）的 n0 恒在最前，
                // n1 的右边框（x ∈ [76, 80)）必须被 n0 的填充覆盖
                let y = 5;
                let overlap = pixel(&pixels, 78, y);
                let plain_fill = pixel(&pixels, 194, y);
                assert_eq!(
                    overlap, plain_fill,
                    "帧 {frame}：重叠区应显示最前音符（索引最小者）的纯填充而非后画者描边"
                );
                // 预览层级：预览音符（白色 70% alpha）恒覆盖主音轨填充
                let preview_pixel = pixel(&pixels, 182, y);
                assert_ne!(
                    preview_pixel, plain_fill,
                    "帧 {frame}：预览音符未覆盖主音轨（预览层未生效）"
                );
                baseline = Some(pixels);
            }
            Some(expected) => {
                assert_eq!(
                    pixels.len(),
                    expected.len(),
                    "帧 {frame}：回读像素数量不一致"
                );
                if let Some(diff) = pixels.iter().zip(expected.iter()).position(|(a, b)| a != b) {
                    let x = (diff as u32 / 4) % TEST_W;
                    let y = (diff as u32 / 4) / TEST_W;
                    panic!(
                        "帧 {frame}：绘制顺序改变导致像素差异 @({x},{y})：{:?} vs {:?}（重叠区描边不应随 cull 输出顺序变化）",
                        pixel(&pixels, x, y),
                        pixel(expected, x, y)
                    );
                }
            }
        }
    }

    assert!(
        baseline.is_some(),
        "至少需要渲染一帧基线；ORDER_FRAMES 不得为 0"
    );
}

/// chunk 基准折叠的构图验证（跨 chunk 索引别名回归）：
/// 两个 note 渲染器当「两个 chunk」用——P 的基准 `chunk_start = 0`、音符本地
/// 索引 100（深度 = 区底 + 100）；Q 的基准 `chunk_start = DEPTH_REGION_SLOTS - 1`、
/// 音符本地索引 50（全局索引超出槽数 → 深度饱和到区顶，远大于 P）。
/// P 先画、Q 后画（Q 的绘制顺序在后）。binding 3 生效时，Q 的全局索引远大于 P
/// → Q 深度更大（更靠后）→ LessEqual 拒绝 Q，最终画面与「只画 P」逐位一致；
/// binding 3 未生效（旧局部索引）时，Q 用本地 50 < P 的 100 → Q 反超在前 →
/// 画面变成 Q 的颜色。本验证不依赖裁剪语义，直接锁定
/// `chunk_start + 本地可见索引` 在 GPU 上真实参与深度。
#[test]
fn test_chunk_start_is_folded_into_depth() {
    let _serial = lock_depth_gpu();
    let (device, queue) = crate::pipeline::test_device();
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;

    // 适配器能力门控：软渲染器压平亚 2^-24 深度时，本断言不可判定（见探针说明）
    if !adapter_resolves_region_depth(&device, &queue, format) {
        eprintln!("跳过 test_chunk_start_is_folded_into_depth：适配器无法分辨区域深度（软渲染）");
        return;
    }

    let red = [1.0, 0.0, 0.0, 1.0];
    let green = [0.0, 1.0, 0.0, 1.0];

    // P：101 个实例，可见索引 100 → 深度 = 区底 + 100
    let mut notes_p = vec![crate::NoteInstance::new(0.0, 60, 100.0, red, 1)];
    notes_p.resize(101, crate::NoteInstance::new(0.0, 60, 100.0, red, 1));
    let mut p = NoteRenderer::new(&device, &queue, format);
    p.upload_instances(&notes_p, &device, &queue);
    write_camera(&p, &queue);
    write_visible_order(&p, &queue, &[100]);

    // Q：51 个实例，可见索引 50；chunk 基准拉到槽数上限 → 深度饱和到区顶
    let mut notes_q = vec![crate::NoteInstance::new(0.0, 60, 100.0, green, 1)];
    notes_q.resize(51, crate::NoteInstance::new(0.0, 60, 100.0, green, 1));
    let mut q = NoteRenderer::new(&device, &queue, format);
    q.upload_instances(&notes_q, &device, &queue);
    write_camera(&q, &queue);
    write_visible_order(&q, &queue, &[50]);
    let uniform = CullUniform {
        instance_count: 51,
        chunk_start: DEPTH_REGION_SLOTS - 1,
        chunk_count: 51,
        _padding: 0,
    };
    queue.write_buffer(
        q.cull_uniform_buffer.inner(),
        0,
        bytemuck::bytes_of(&uniform),
    );

    let color_texture = make_color_texture(&device, format);
    let depth_texture = make_depth_texture(&device);
    let color_view = color_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

    let render_all = |renderers: &[&NoteRenderer]| {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("note_chunk_fold_pass"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("note_chunk_fold_pass"),
                color_attachments: &[Some(color_attachment(&color_view))],
                depth_stencil_attachment: Some(depth_attachment(&depth_view)),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            for renderer in renderers {
                renderer.draw(&mut pass, true, None);
            }
        }
        queue.submit(Some(encoder.finish()));
        readback_pixels(&device, &queue, &color_texture)
    };

    let pixels_p = render_all(&[&p]);
    let pixels_q = render_all(&[&q]);
    assert_ne!(
        pixel(&pixels_p, 50, 5),
        pixel(&pixels_q, 50, 5),
        "红/绿参考渲染必须可区分（测试装置自检）"
    );
    let pixels_pq = render_all(&[&p, &q]);
    assert_eq!(
        pixels_pq, pixels_p,
        "chunk_start 未参与深度：后画的 Q（本地索引 50）反超了 P（本地索引 100）"
    );
}

/// 视频导出（无 depth attachment）回归：音符不得缺失或被错误裁剪——
/// 同一场景在有 depth / 无 depth 两条管线下的覆盖掩码必须完全一致。
#[test]
fn test_depthless_pass_keeps_note_coverage() {
    let _serial = lock_depth_gpu();
    let (device, queue) = crate::pipeline::test_device();
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let notes = onion_scene();
    let order: Vec<u32> = (0..notes.len() as u32).collect();

    let mut with_depth = NoteRenderer::new(&device, &queue, format);
    with_depth.upload_instances(&notes, &device, &queue);
    write_camera(&with_depth, &queue);
    write_visible_order(&with_depth, &queue, &order);

    let mut no_depth = NoteRenderer::new_without_depth(&device, &queue, format);
    no_depth.upload_instances(&notes, &device, &queue);
    write_camera(&no_depth, &queue);
    write_visible_order(&no_depth, &queue, &order);

    let color_with_depth = make_color_texture(&device, format);
    let depth_texture = make_depth_texture(&device);
    let view_with_depth = color_with_depth.create_view(&wgpu::TextureViewDescriptor::default());
    let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

    let color_no_depth = make_color_texture(&device, format);
    let view_no_depth = color_no_depth.create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("note_depthless_regression_encoder"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("note_with_depth_pass"),
            color_attachments: &[Some(color_attachment(&view_with_depth))],
            depth_stencil_attachment: Some(depth_attachment(&depth_view)),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        with_depth.draw(&mut pass, true, None);
    }
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("note_no_depth_pass"),
            color_attachments: &[Some(color_attachment(&view_no_depth))],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        no_depth.draw(&mut pass, true, None);
    }
    queue.submit(Some(encoder.finish()));

    let pixels_with_depth = readback_pixels(&device, &queue, &color_with_depth);
    let pixels_no_depth = readback_pixels(&device, &queue, &color_no_depth);
    let mask_with_depth = coverage_mask(&pixels_with_depth);
    let mask_no_depth = coverage_mask(&pixels_no_depth);

    let covered = mask_with_depth.iter().filter(|c| **c).count();
    assert!(covered > 0, "有 depth 参照渲染未画出任何音符");
    assert_eq!(
        mask_no_depth.iter().filter(|c| **c).count(),
        covered,
        "无 depth 路径的音符覆盖像素数与有 depth 路径不一致（音符缺失/被错误裁剪）"
    );
    if let Some(diff) = mask_with_depth
        .iter()
        .zip(mask_no_depth.iter())
        .position(|(a, b)| a != b)
    {
        let x = (diff as u32) % TEST_W;
        let y = (diff as u32) / TEST_W;
        panic!("无 depth 路径覆盖掩码与有 depth 路径不一致 @({x},{y})");
    }
}

/// 洋葱皮无 depth 变体必须与无 depth 的 RenderPass 兼容：
/// 修复前 `new_onion_skin` 硬编码 needs_depth=true，在无 depth pass 中
/// `set_pipeline` 会触发 wgpu 校验错误（整条命令缓冲被丢弃）。
#[test]
fn test_onion_skin_depthless_pipeline_is_pass_compatible() {
    let _serial = lock_depth_gpu();
    let (device, queue) = crate::pipeline::test_device();
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let notes = onion_scene();

    let mut onion = NoteRenderer::new_onion_skin_without_depth(&device, &queue, format);
    onion.set_view_state(&queue, 1, &[]);
    onion.upload_instances(&notes, &device, &queue);
    write_camera(&onion, &queue);
    write_visible_order(&onion, &queue, &(0..notes.len() as u32).collect::<Vec<_>>());
    assert!(
        onion.last_upload_count() > 0,
        "洋葱皮渲染器必须实际上传实例，否则 draw 提前返回、校验错误不会暴露"
    );

    let color_texture = make_color_texture(&device, format);
    let color_view = color_texture.create_view(&wgpu::TextureViewDescriptor::default());

    device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("note_onion_depthless_encoder"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("note_onion_depthless_pass"),
            color_attachments: &[Some(color_attachment(&color_view))],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        onion.draw(&mut pass, true, None);
    }
    queue.submit(Some(encoder.finish()));
    // 有界等待（禁止 timeout: None 无限阻塞）
    let _ = device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(std::time::Duration::from_secs(30)),
    });
    let error = futures::executor::block_on(device.pop_error_scope());
    assert!(
        error.is_none(),
        "无 depth RenderPass 中绘制洋葱皮触发了 wgpu 校验错误：{error:?}"
    );

    // 静音轨（非主音轨）必须完全不产生片元：覆盖掩码为空
    let mut muted_onion = NoteRenderer::new_onion_skin_without_depth(&device, &queue, format);
    // current_track = 1（主音轨 = track_enc 1）；轨道索引 1/2 静音
    // → track_enc 2（key 58）/ track_enc 3（key 56）必须被裁剪，仅主音轨可见
    muted_onion.set_view_state(&queue, 1, &[1, 2]);
    muted_onion.upload_instances(&notes, &device, &queue);
    write_camera(&muted_onion, &queue);
    write_visible_order(
        &muted_onion,
        &queue,
        &(0..notes.len() as u32).collect::<Vec<_>>(),
    );
    let muted_texture = make_color_texture(&device, format);
    let muted_view = muted_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("note_onion_muted_encoder"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("note_onion_muted_pass"),
            color_attachments: &[Some(color_attachment(&muted_view))],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        muted_onion.draw(&mut pass, true, None);
    }
    queue.submit(Some(encoder.finish()));
    let muted_pixels = readback_pixels(&device, &queue, &muted_texture);
    let clear = background_pixel(&muted_pixels);
    // 洋葱皮轨道（key 58 → y ∈ [20, 30)、key 56 → y ∈ [40, 50)）在无 depth 路径下
    // 也必须被静音裁剪掉，不得因「z=2.0 无 depth attachment 不裁剪」而误绘
    for y in 20..50u32 {
        for x in 0..TEST_W {
            assert_eq!(
                pixel(&muted_pixels, x, y),
                clear,
                "静音轨在无 depth 路径下仍被绘制 @({x},{y})"
            );
        }
    }
}
