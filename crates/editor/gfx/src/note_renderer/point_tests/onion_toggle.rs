//! 洋葱皮全局开关（`ViewState.onion_visible`）的像素级契约。
//!
//! 用户需求：「点击按钮后显示全部音轨的洋葱皮，关闭就彻底关闭；GPU 模式下
//! 关闭不卸载数据，只不显示」。本文件用**真实 GPU 回读**锁定四件事：
//! 1. 开关关闭 → 非主音轨实例一个片元都不落（主音轨照常显示）；
//! 2. 重新打开 → 同一份实例缓冲（零重传）立刻恢复洋葱皮；
//! 3. 关闸在 **compute cull** 阶段就生效（不只是 VS 退化），裁掉的实例
//!    连顶点着色器都不跑、也不进 indirect 实例数；
//! 4. 裁剪 shader 的闸门判据与洋葱皮 VS 同口径（纯源码断言，防静默丢失）。
//!
//! 独立文件以保持 `point_tests.rs` < 400 行；渲染与断言辅助复用父模块。

use crate::NoteInstance;
use crate::note_renderer::NoteRenderer;

use super::{
    FORMAT, NOTE_COLOR, color_attachment, covered_rows, depth_attachment, encode, expected_row,
    key_row_covered, make_textures, normal_camera, read_pixels, render_points,
};

/// **契约 5**：开关关闭时只画主音轨，实例数据不卸载、重新打开即时恢复。
#[test]
fn test_point_mode_hides_onion_when_disabled() {
    let notes = vec![
        // 主音轨（track_enc 1 = current_track）
        NoteInstance::new(0.0, 90, 120.0, NOTE_COLOR, encode(1, 2)),
        // 洋葱皮轨（track_enc 2，未静音）：开关关闭时必须不可见
        NoteInstance::new(0.0, 70, 120.0, NOTE_COLOR, encode(2, 2)),
    ];

    let hidden_rows = covered_rows(&render_points(&notes, 1, &[], false));
    assert!(
        key_row_covered(&hidden_rows, 90),
        "关闭洋葱皮后主音轨必须照常绘制（key 90 → 期望行 {}）",
        expected_row(90)
    );
    assert!(
        !key_row_covered(&hidden_rows, 70),
        "关闭洋葱皮后非主音轨不得绘制（key 70 → 期望行 {}）",
        expected_row(70)
    );

    // 同一份实例数据（零重传）只翻开关：洋葱皮必须立刻回来
    let shown_rows = covered_rows(&render_points(&notes, 1, &[], true));
    assert!(
        key_row_covered(&shown_rows, 70),
        "重新打开洋葱皮后非主音轨必须恢复显示（数据未重传，key 70 → 期望行 {}）",
        expected_row(70)
    );
}

/// `normal_camera`（zoom_y = 10）下某键音符的中心行
fn cull_row(key: u8) -> u32 {
    ((127.0 - f32::from(key)) * 10.0 + 5.0) as u32
}

/// 回读 `indirect_buffer` 槽位 0 的 `instance_count`（cull 真正放行的实例数）。
///
/// 槽位布局 `{ vertex_count, instance_count, first_vertex, first_instance }`
/// ⇒ `instance_count` 位于字节偏移 4。
fn read_cull_instance_count(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &NoteRenderer,
) -> u32 {
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("onion_cull_count_staging"),
        size: 16,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("onion_cull_count_copy"),
    });
    encoder.copy_buffer_to_buffer(renderer.indirect_buffer.inner(), 0, &staging, 0, 16);
    queue.submit(Some(encoder.finish()));

    let slice = staging.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let _ = device.poll(wgpu::PollType::Poll);
        match rx.try_recv() {
            Ok(Ok(())) => break,
            Ok(Err(e)) => panic!("cull 计数回读失败: {e:?}"),
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                assert!(
                    std::time::Instant::now() < deadline,
                    "cull 计数回读 30s 未就绪"
                );
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => panic!("cull 计数回调丢失"),
        }
    }
    let data = slice.get_mapped_range().to_vec();
    staging.unmap();
    u32::from_le_bytes([data[4], data[5], data[6], data[7]])
}

/// 走**生产默认路径**渲染一帧并回读：`prepare_pass` 的 compute 裁剪 +
/// `draw` 的间接绘制（与 `render_points` 的区别是这里不绕开 cull）。
///
/// 返回 `(像素, cull 放行的实例数)`。
fn render_cull(
    notes: &[NoteInstance],
    current_track: u32,
    muted: &[usize],
    onion_visible: bool,
) -> (Vec<u8>, u32) {
    let (device, queue) = crate::test_gpu::shared_device();

    let mut renderer =
        NoteRenderer::new_onion_skin(&device, &queue, FORMAT, crate::GpuEpoch::INITIAL);
    renderer.set_view_state(&queue, current_track, muted, onion_visible);
    renderer.upload_instances(notes, &device, &queue);

    let (color, _depth, color_view, depth_view) = make_textures(&device, "onion_cull_color");
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("onion_cull_encoder"),
    });
    renderer.prepare_pass(&mut encoder, normal_camera(), &queue);
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("onion_cull_pass"),
            color_attachments: &[Some(color_attachment(&color_view))],
            depth_stencil_attachment: Some(depth_attachment(&depth_view)),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        renderer.draw(&mut pass, true, None);
    }
    queue.submit(Some(encoder.finish()));

    let cull_count = read_cull_instance_count(&device, &queue, &renderer);
    (read_pixels(&device, &queue, &color), cull_count)
}

/// **契约 5b**：关闸在 **compute cull** 阶段生效，不只是 VS 退化。
///
/// 早前实现只在 VS 里把隐藏实例压成 4 个重合顶点（零面积 → 零片元），
/// 像素正确但**每个隐藏实例仍要跑一遍 VS**。现在 cull 先按轨道闸门过滤：
/// 被拒实例不写可见索引、不进 indirect 实例数，整条渲染链路都省掉。
///
/// 像素断言只能说明「没画出来」（VS 退化也满足），所以本测试额外回读
/// `indirect_buffer.instance_count`——那是「谁都别跑」的唯一硬证据。
#[test]
fn test_cull_path_drops_onion_when_disabled() {
    let notes = vec![
        // 主音轨（track_enc 1 = current_track），key 110 → 行 175
        NoteInstance::new(0.0, 110, 120.0, NOTE_COLOR, encode(1, 2)),
        // 洋葱皮轨（track_enc 2），key 120 → 行 75
        NoteInstance::new(0.0, 120, 120.0, NOTE_COLOR, encode(2, 2)),
    ];

    let (hidden, hidden_count) = render_cull(&notes, 1, &[], false);
    let hidden = covered_rows(&hidden);
    assert!(
        hidden.contains(&cull_row(110)),
        "关闭洋葱皮后主音轨必须照常绘制（key 110 → 行 {}），实覆盖行 {:?}",
        cull_row(110),
        hidden
    );
    assert!(
        !hidden.contains(&cull_row(120)),
        "关闭洋葱皮后非主音轨必须被 cull 丢弃（key 120 → 行 {}），实覆盖行 {:?}",
        cull_row(120),
        hidden
    );
    assert_eq!(
        hidden_count, 1,
        "关闭洋葱皮后 cull 只应放行主音轨 1 个实例（隐藏实例连 VS 都不该跑）"
    );

    let (shown, shown_count) = render_cull(&notes, 1, &[], true);
    let shown = covered_rows(&shown);
    assert!(
        shown.contains(&cull_row(120)),
        "打开洋葱皮后非主音轨必须恢复显示（key 120 → 行 {}），实覆盖行 {:?}",
        cull_row(120),
        shown
    );
    assert_eq!(
        shown_count, 2,
        "打开洋葱皮后 cull 应放行 2 个实例（主音轨 + 洋葱皮）"
    );
}

/// **契约 5c**：裁剪 shader 里必须真的写着轨道闸门。
///
/// 像素与实例数都只覆盖运行期行为；「闸门写在 cull 里、且带预览哨兵豁免」
/// 用源码断言钉死，防止后续重构把 binding 5 或判据悄悄删掉。
#[test]
fn test_cull_shaders_carry_track_gate() {
    const CULLS: [(&str, &str); 2] = [
        ("cull", include_str!("../../shaders/cull.wgsl")),
        (
            "cull_vertical",
            include_str!("../../shaders/cull_vertical.wgsl"),
        ),
    ];
    for (label, source) in CULLS {
        for needle in [
            "@group(0) @binding(5) var<uniform> view_state: ViewState;",
            "onion_visible: u32,",
            "fn track_visible(border_width: u32) -> bool {",
            "if (view_state.current_track == 0u) {",
            "&& track_visible(instance.border_width)",
        ] {
            assert!(
                source.contains(needle),
                "{label}.wgsl 缺少轨道闸门片段：{needle}"
            );
        }
        // 预览哨兵豁免：预览层（note.wgsl）不走轨道语义，误杀会让绘制/hover/i2m
        // 预览整体消失（预览渲染器从不调用 set_view_state）。
        assert!(
            source.contains("const PREVIEW_BORDER_SENTINEL: u32 = 0xFFFFFFFFu;"),
            "{label}.wgsl 缺少预览哨兵豁免"
        );
    }

    // 判据必须与 VS 同口径，否则「可见索引里留下的」与「真正画出的」会错位
    let vs = include_str!("../../shaders/onion_note.wgsl");
    assert!(
        vs.contains("view_state.onion_visible != 0u"),
        "onion_note.wgsl 缺少洋葱皮开关判据"
    );
}
