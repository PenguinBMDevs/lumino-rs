//! 洋葱皮渲染性能基准测试（完整 CPU+GPU 管线，wgpu 无头模式）
//!
//! 精确复制主程序完整渲染管线：
//!   1. CPU: 视口哈希检测 + 主音轨全量 iter + 洋葱皮视口过滤 + SwappableBuffer swap
//!   2. GPU: NoteRenderer::prepare_notes (upload + compute cull)
//!   3. GPU: RenderPass (grid + notes draw)
//!   4. GPU: queue.submit + device.poll (等待完成)
//! 直接调用主程序逻辑，不单独写渲染逻辑。

use std::collections::HashMap;
use std::hint::black_box;
use std::time::Instant;

use lumino_gfx::{
    CameraParams, CameraUniform, GridRenderer, NoteInstance, NoteRenderer, SwappableBuffer,
};
use lumino_midi_loader::MidiDocument;
use lumino_ui::editor::Editor;
use lumino_ui::editor::note::Note;
use lumino_ui::host::RenderCache;

const CANVAS_WIDTH: f32 = 1920.0;
const CANVAS_HEIGHT: f32 = 1080.0;
const KEYBOARD_WIDTH: f32 = 60.0;
const RULER_HEIGHT: f32 = 30.0;
const ZOOM_X: f32 = 0.05;
const ZOOM_Y: f32 = 20.0;
const DEFAULT_NOTE_COLOR: [f32; 4] = [0.2, 0.5, 1.0, 0.9];

const MAIN_NOTES: usize = 1_000_000;
const ONION_TRACKS: usize = 100;
const ONION_NOTES_PER_TRACK: usize = 100_000;
const TOTAL_TICKS: u32 = 10_000_000;

fn write_vlq(buf: &mut Vec<u8>, mut value: u32) {
    let mut bytes = Vec::with_capacity(4);
    bytes.push((value & 0x7F) as u8);
    value >>= 7;
    while value > 0 {
        bytes.push((value & 0x7F) as u8 | 0x80);
        value >>= 7;
    }
    buf.extend(bytes.into_iter().rev());
}

fn create_test_midi(path: &std::path::Path) {
    let num_tracks = 1 + ONION_TRACKS;
    let ppqn: u16 = 480;

    eprintln!(
        "生成 MIDI 数据: 1 主音轨 x {} + {} 洋葱轨 x {} = {} 总音符",
        MAIN_NOTES,
        ONION_TRACKS,
        ONION_NOTES_PER_TRACK,
        MAIN_NOTES + ONION_TRACKS * ONION_NOTES_PER_TRACK
    );

    let mut midi_data = Vec::with_capacity(90_000_000);
    midi_data.extend_from_slice(b"MThd");
    midi_data.extend_from_slice(&6u32.to_be_bytes());
    midi_data.extend_from_slice(&1u16.to_be_bytes());
    midi_data.extend_from_slice(&(num_tracks as u16).to_be_bytes());
    midi_data.extend_from_slice(&ppqn.to_be_bytes());

    let gen_start = Instant::now();
    {
        let mut track_data = Vec::with_capacity(MAIN_NOTES * 8 + 32);
        write_vlq(&mut track_data, 0);
        track_data.extend_from_slice(&[0xFF, 0x03, 5, b'T', b'r', b'a', b'c', b'k', 0]);
        let tick_step = TOTAL_TICKS / MAIN_NOTES as u32;
        for i in 0..MAIN_NOTES {
            let key = 60 + (i % 48) as u8;
            let length = tick_step / 2;
            write_vlq(&mut track_data, if i == 0 { 0 } else { tick_step });
            track_data.extend_from_slice(&[0x90, key, 100]);
            write_vlq(&mut track_data, length);
            track_data.extend_from_slice(&[0x80, key, 0]);
        }
        write_vlq(&mut track_data, 0);
        track_data.extend_from_slice(&[0xFF, 0x2F, 0x00]);
        midi_data.extend_from_slice(b"MTrk");
        midi_data.extend_from_slice(&(track_data.len() as u32).to_be_bytes());
        midi_data.extend_from_slice(&track_data);
    }
    let tick_step = TOTAL_TICKS / ONION_NOTES_PER_TRACK as u32;
    for track_idx in 0..ONION_TRACKS {
        let mut track_data = Vec::with_capacity(ONION_NOTES_PER_TRACK * 8 + 32);
        let name = format!("Onion Track {}", track_idx + 1);
        write_vlq(&mut track_data, 0);
        track_data.extend_from_slice(&[0xFF, 0x03]);
        write_vlq(&mut track_data, name.len() as u32);
        track_data.extend_from_slice(name.as_bytes());
        for i in 0..ONION_NOTES_PER_TRACK {
            let key = 36 + ((track_idx * 12 + i) % 48) as u8;
            let length = tick_step / 3;
            write_vlq(&mut track_data, if i == 0 { 0 } else { tick_step });
            track_data.extend_from_slice(&[0x90, key, 80 + (track_idx % 40) as u8]);
            write_vlq(&mut track_data, length);
            track_data.extend_from_slice(&[0x80, key, 0]);
        }
        write_vlq(&mut track_data, 0);
        track_data.extend_from_slice(&[0xFF, 0x2F, 0x00]);
        midi_data.extend_from_slice(b"MTrk");
        midi_data.extend_from_slice(&(track_data.len() as u32).to_be_bytes());
        midi_data.extend_from_slice(&track_data);
    }
    eprintln!(
        "MIDI 数据生成完成: {:?}, 大小={}MB",
        gen_start.elapsed(),
        midi_data.len() / 1024 / 1024
    );
    std::fs::write(path, &midi_data).expect("写入测试 MIDI 文件失败");
}

#[test]
fn onion_skin_benchmark() {
    // ========== 1. 创建测试数据 ==========
    let tmp_dir = std::env::temp_dir().join("lumino_onion_skin_bench");
    let _ = std::fs::create_dir_all(&tmp_dir);
    let midi_path = tmp_dir.join("bench_test_11m.mid");

    eprintln!("创建测试 MIDI 文件...");
    let start = Instant::now();
    create_test_midi(&midi_path);
    eprintln!("创建完成: {:?}", start.elapsed());

    eprintln!("加载 MIDI 文档...");
    let load_start = Instant::now();
    let doc = MidiDocument::from_notes_file(&midi_path, None).expect("加载 MIDI 文档失败");
    eprintln!(
        "加载完成: {:?}, 音轨数={}",
        load_start.elapsed(),
        doc.track_count
    );

    // ========== 2. 创建 Editor + 加载主音轨 ==========
    let mut editor = Editor::new();
    let main_tick_step = (TOTAL_TICKS as usize) / MAIN_NOTES;
    eprintln!("加载主音轨 {} 个音符到编辑器...", MAIN_NOTES);
    let load_notes_start = Instant::now();
    for i in 0..MAIN_NOTES {
        let tick = (i * main_tick_step) as f32;
        let key = (60 + (i % 48)) as u16;
        let length = (main_tick_step / 2) as f32;
        editor.editor_state.data.notes.push_back(
            Note::new(tick, key, length)
                .with_velocity(100)
                .with_channel(0),
        );
    }
    eprintln!("主音轨加载完成: {:?}", load_notes_start.elapsed());

    editor.editor_state.data.document = Some(std::sync::Arc::new(doc));
    editor.editor_state.view.zoom_x = ZOOM_X;
    editor.editor_state.view.zoom_y = ZOOM_Y;
    editor.editor_state.view.scroll_x = 0.0;
    editor.editor_state.view.scroll_y = 0.0;
    editor.editor_state.view.keyboard_width = KEYBOARD_WIDTH;
    editor.editor_state.view.visible_key_count = 128;
    editor.editor_state.view.ppq = 480;
    editor.editor_state.canvas.size_x = CANVAS_WIDTH;
    editor.editor_state.canvas.size_y = CANVAS_HEIGHT;

    editor.enable_onion_skin();
    editor.set_onion_skin_show_all(true);

    let mut onion_states = HashMap::new();
    for i in 1..=ONION_TRACKS {
        onion_states.insert(i, true);
    }
    editor.editor_state.data.current_track = 0;

    // ========== 3. 创建 SwappableBuffer ==========
    let note_buffer = SwappableBuffer::<NoteInstance>::new(MAIN_NOTES + 10000);
    let mut note_viewport_hash: u64 = 0;

    // ========== 4. 创建 wgpu 无头设备 ==========
    eprintln!("创建 wgpu 无头设备...");
    let wgpu_start = Instant::now();

    let rt = tokio::runtime::Runtime::new().expect("创建tokio运行时失败");
    let (device, queue) = rt.block_on(async {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
            })
            .await
            .expect("创建 wgpu adapter 失败（无 GPU？）");
        adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("headless_bench_device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                ..Default::default()
            })
            .await
            .expect("创建 wgpu device 失败")
    });
    eprintln!("wgpu 设备创建完成: {:?}", wgpu_start.elapsed());

    let format = wgpu::TextureFormat::Bgra8UnormSrgb;
    let mut note_renderer = NoteRenderer::new(&device, &queue, format);
    let mut grid_renderer = GridRenderer::new(&device, format);

    // 创建渲染目标纹理
    let render_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("bench_render_target"),
        size: wgpu::Extent3d {
            width: 1920,
            height: 1080,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let render_view = render_texture.create_view(&wgpu::TextureViewDescriptor::default());

    // 创建深度纹理
    let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("bench_depth"),
        size: wgpu::Extent3d {
            width: 1920,
            height: 1080,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

    // 准备网格（一次，视口不变时不需要重准备）
    grid_renderer.prepare(
        &queue,
        (CANVAS_WIDTH, CANVAS_HEIGHT),
        0.0,
        0.0,
        ZOOM_X,
        ZOOM_Y,
        KEYBOARD_WIDTH,
        RULER_HEIGHT,
        [0.1, 0.1, 0.12, 1.0],   // bg
        [0.08, 0.08, 0.1, 1.0],  // black_key
        [0.3, 0.3, 0.4, 1.0],    // bar
        [0.2, 0.2, 0.3, 1.0],    // beat
        [0.15, 0.15, 0.2, 1.0],  // half_beat
        [0.12, 0.12, 0.15, 1.0], // grid
        [0.25, 0.25, 0.35, 1.0], // key_line
        480.0,
        127.0,
        0.0,
        0.0,
    );

    // ========== 5. 调试信息 ==========
    {
        let end = (CANVAS_WIDTH - KEYBOARD_WIDTH) / ZOOM_X;
        let doc = editor.editor_state.data.document.as_ref().expect("获取编辑器文档引用失败");
        let mut raw = 0usize;
        for tid in 1..=ONION_TRACKS as u16 {
            raw += doc.get_track_notes_in_range(tid, 0.0, end).len();
        }
        eprintln!(
            "每帧处理: 主音轨 {} 全量 + 洋葱皮 ~{} 原始 | GPU compute cull + draw",
            MAIN_NOTES, raw,
        );
    }

    // ========== 6. 预热 ==========
    eprintln!("预热...");
    for frame in 0..5 {
        let scroll_tick = (frame as f32) * 100_000.0;
        editor.editor_state.view.scroll_x = scroll_tick * ZOOM_X;
        let _visible_end = (scroll_tick * ZOOM_X + CANVAS_WIDTH - KEYBOARD_WIDTH) / ZOOM_X;

        let hash = RenderCache::compute_viewport_hash(
            editor.editor_state.view.scroll_x,
            editor.editor_state.view.scroll_y,
            ZOOM_X,
            ZOOM_Y,
            CANVAS_WIDTH,
            CANVAS_HEIGHT,
            editor.editor_state.view.visible_key_count,
        );

        let onion: Vec<NoteInstance> = Vec::new();
        let instances = unsafe { note_buffer.write_buffer() };
        instances.clear();
        instances.reserve(MAIN_NOTES + onion.len() + 1);
        for note in editor.editor_state.data.notes.iter() {
            instances.push(NoteInstance::new(
                note.tick,
                note.key as f32,
                note.length,
                DEFAULT_NOTE_COLOR,
            ));
        }
        instances.extend(onion);
        note_buffer.swap();
        note_viewport_hash = hash;

        // GPU 预热
        let camera = CameraUniform::new(CameraParams {
            scroll: [editor.editor_state.view.scroll_x, 0.0],
            zoom: [ZOOM_X, ZOOM_Y],
            viewport: [CANVAS_WIDTH, CANVAS_HEIGHT],
            offset: [0.0, 0.0],
            keyboard_width: KEYBOARD_WIDTH,
            ruler_height: RULER_HEIGHT,
            max_key_index: 127.0,
        });
        let note_instances = unsafe { note_buffer.read_buffer() };
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("warmup_encoder"),
        });
        if !note_instances.is_empty() {
            note_renderer.prepare_notes(&mut encoder, note_instances, &device, &queue, camera);
        }
        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("warmup_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &render_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        grid_renderer.draw(&mut rp, 1);
        note_renderer.draw(&mut rp, !note_instances.is_empty(), None);
        drop(rp);
        queue.submit(std::iter::once(encoder.finish()));
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        black_box(note_viewport_hash);
    }

    // ========== 7. 基准测试 ==========
    const NUM_FRAMES: u32 = 50;
    const TICK_SPEED: f32 = 5_000.0;

    eprintln!(
        "\n开始基准测试: {} 帧, 每帧移动 {} ticks\n\
         CPU: 主音轨 {} 全量 iter + 洋葱皮视口过滤 + SwappableBuffer swap\n\
         GPU: upload + compute cull + render pass + device.poll",
        NUM_FRAMES, TICK_SPEED, MAIN_NOTES,
    );

    let bench_start = Instant::now();
    let mut total_instances: usize = 0;
    let mut rebuild_count: u32 = 0;

    for frame in 0..NUM_FRAMES {
        let scroll_tick = (frame as f32) * TICK_SPEED;
        editor.editor_state.view.scroll_x = scroll_tick * ZOOM_X;
        let _visible_end = (scroll_tick * ZOOM_X + CANVAS_WIDTH - KEYBOARD_WIDTH) / ZOOM_X;

        // === CPU: update_note_data_for_wgpu_thread ===
        let note_index_dirty = editor.spatial.note_index_dirty.get();
        let note_data_changed = note_index_dirty || unsafe { note_buffer.read_buffer().is_empty() };

        let current_hash = RenderCache::compute_viewport_hash(
            editor.editor_state.view.scroll_x,
            editor.editor_state.view.scroll_y,
            ZOOM_X,
            ZOOM_Y,
            CANVAS_WIDTH,
            CANVAS_HEIGHT,
            editor.editor_state.view.visible_key_count,
        );
        let viewport_changed = current_hash != note_viewport_hash;

        if note_data_changed || viewport_changed {
            rebuild_count += 1;

            let onion: Vec<NoteInstance> = Vec::new();
            let instances = unsafe { note_buffer.write_buffer() };
            instances.clear();
            instances.reserve(MAIN_NOTES + onion.len() + 1);
            for note in editor.editor_state.data.notes.iter() {
                instances.push(NoteInstance::new(
                    note.tick,
                    note.key as f32,
                    note.length,
                    DEFAULT_NOTE_COLOR,
                ));
            }
            instances.extend(onion);
            note_buffer.swap();
            note_viewport_hash = current_hash;
            if note_index_dirty {
                editor.spatial.note_index_dirty.set(false);
            }
        }

        // === GPU: prepare_note_renderer + execute_render_pass ===
        let camera = CameraUniform::new(CameraParams {
            scroll: [editor.editor_state.view.scroll_x, 0.0],
            zoom: [ZOOM_X, ZOOM_Y],
            viewport: [CANVAS_WIDTH, CANVAS_HEIGHT],
            offset: [0.0, 0.0],
            keyboard_width: KEYBOARD_WIDTH,
            ruler_height: RULER_HEIGHT,
            max_key_index: 127.0,
        });

        let note_instances = unsafe { note_buffer.read_buffer() };
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("bench_encoder"),
        });

        if !note_instances.is_empty() {
            note_renderer.prepare_notes(&mut encoder, note_instances, &device, &queue, camera);
        }

        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("bench_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &render_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        grid_renderer.draw(&mut rp, 1);
        note_renderer.draw(&mut rp, !note_instances.is_empty(), None);
        drop(rp);

        queue.submit(std::iter::once(encoder.finish()));
        device.poll(wgpu::PollType::wait_indefinitely());

        total_instances += note_instances.len();
        black_box(note_viewport_hash);
    }

    let bench_elapsed = bench_start.elapsed();
    let avg_frame_time = bench_elapsed / NUM_FRAMES;
    let fps = NUM_FRAMES as f64 / bench_elapsed.as_secs_f64();
    let avg_per_frame = total_instances / NUM_FRAMES as usize;

    eprintln!("\n========== 洋葱皮渲染性能基准测试结果（CPU+GPU 完整管线） ==========");
    eprintln!("总帧数:            {}", NUM_FRAMES);
    eprintln!("重建帧数:          {} (视口变化触发)", rebuild_count);
    eprintln!("总耗时:            {:?}", bench_elapsed);
    eprintln!("平均每帧耗时:      {:?}", avg_frame_time);
    eprintln!("FPS:               {:.1}", fps);
    eprintln!("平均实例数/帧:     {}", avg_per_frame);
    eprintln!("  ├─ 主音轨:       {}", MAIN_NOTES);
    eprintln!(
        "  └─ 洋葱皮:       {} (avg)",
        avg_per_frame.saturating_sub(MAIN_NOTES)
    );
    eprintln!(
        "可见范围宽度:      {:.0} ticks",
        (CANVAS_WIDTH - KEYBOARD_WIDTH) / ZOOM_X
    );
    eprintln!(
        "总滚动距离:        {:.0} ticks",
        (NUM_FRAMES as f32) * TICK_SPEED
    );
    eprintln!("管线组件:");
    eprintln!("  ├─ CPU: 视口哈希计算 + 变化检测");
    eprintln!("  ├─ CPU: 主音轨 {} 全量 iter → NoteInstance", MAIN_NOTES);
    eprintln!("  ├─ CPU: 洋葱皮视口过滤 (get_all_onion_skin_instances_in_range)");
    eprintln!("  ├─ CPU: SwappableBuffer write_buffer + swap");
    eprintln!("  ├─ GPU: NoteRenderer::prepare_notes (upload + compute cull)");
    eprintln!("  ├─ GPU: GridRenderer::draw");
    eprintln!("  ├─ GPU: NoteRenderer::draw (indirect)");
    eprintln!("  └─ GPU: queue.submit + device.poll (Wait)");
    eprintln!("============================================================\n");

    assert!(
        avg_per_frame >= MAIN_NOTES,
        "应该有至少 {} 个主音轨实例",
        MAIN_NOTES
    );
    eprintln!("基准测试完成: FPS = {:.1}", fps);
    eprintln!("(主音轨 100W 全量 + 洋葱皮 1000W 快速滚动 | wgpu 无头模式)");
}
