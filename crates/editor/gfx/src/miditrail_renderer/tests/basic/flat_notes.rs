use super::*;

/// A/B：平面模式是盒子模式的严格几何子集（行为零改动的可验证定义）。
///
/// 同实例/同顺序/同管线下，平面三角形逐个等于盒子的顶面三角形
/// （同顶点/同插值/不透明覆盖），故平面非黑像素集 ⊆ 盒子非黑像素集；
/// 琴键/Aura 两边完全一致（同缓冲同实例）。
/// 差异仅来自被删掉的侧面/底面/背面曾覆盖的像素——这正是开关要的外观变化。
/// 双视图循环：平面几何与视图无关（统一顶面），Normal/Top 逐个断言，
/// 防"分视图取面"类回归（曾误取正面压掉 Z 长）。
#[test]
fn test_flat_notes_is_pixel_subset_of_box() {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        .expect("测试需要可用的 wgpu 适配器");
    let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("miditrail_flat_ab_test_device"),
        required_features: adapter.features() & wgpu::Features::default(),
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::default(),
        trace: wgpu::Trace::Off,
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
    }))
    .expect("请求 wgpu 设备失败");

    // 两枚重叠红音符：覆盖画家排序 + 侧面交叠路径。
    let notes = [(480u32, 960u32, 60u8), (600u32, 480u32, 64u8)];
    let manual: Vec<MiditrailNoteGpu> = notes
        .iter()
        .map(|&(s, l, k)| MiditrailNoteGpu {
            key: u32::from(k),
            start_tick: s,
            end_tick: s + l,
            color_packed: pack_color([1.0, 0.0, 0.0, 1.0]),
            track_idx: 0,
            velocity: 100,
            channel: 0,
            _padding: 0,
        })
        .collect();

    for view_mode in [MiditrailViewMode::Normal, MiditrailViewMode::Top] {
        let uniform = MiditrailUniformGpu {
            tick: 480,
            ppq: 480,
            key_count: 128,
            frame_width: 320,
            frame_height: 180,
            kb_height: 20,
            _reserved: 0,
            speed: 1.0,
            param1: 0.0,
            param2: 0.0,
            fps: 60.0,
            z_far_distance: 7.5,
            view_mode,
            ticks_per_second: 960.0,
            _padding1: 0,
        };
        let mut renderer_box = MiditrailRenderer::new(&device);
        renderer_box.flat_notes = false;
        let box_pixels =
            render_and_count_non_black(&device, &queue, &mut renderer_box, &uniform, &manual);

        let mut renderer_quad = MiditrailRenderer::new(&device);
        renderer_quad.flat_notes = true;
        let quad_pixels =
            render_and_count_non_black(&device, &queue, &mut renderer_quad, &uniform, &manual);

        assert!(box_pixels > 0, "{view_mode:?} 盒子模式应渲染出可见内容");
        assert!(quad_pixels > 0, "{view_mode:?} 平面模式应渲染出可见内容");
        assert!(
            quad_pixels <= box_pixels,
            "{view_mode:?} 平面像素集应为盒子子集：quad={quad_pixels} box={box_pixels}"
        );
    }
}
