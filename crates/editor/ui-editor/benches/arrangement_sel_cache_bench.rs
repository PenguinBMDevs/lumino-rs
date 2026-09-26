//! 工程走带选区命中音符：**view 层每帧派生查询**基准
//!
//! # 缺陷背景（2026-09）
//!
//! 走带框选后 `view_arrangement` 与 `view_main_content` 帧耗时飙到 12.9ms /
//! 10.6ms（合计占单帧 96%）。根因不是框选本身贵，而是**选区一旦非空**，
//! `is_empty()` 的 O(1) 早退失效，两个每帧调用点各自全量扫描全文档音符：
//!
//! | 调用点 | 每帧代价 | 真实需要 |
//! |--------|---------|---------|
//! | `view_arrangement` → `arrangement_selected_notes()` | 全量扫描 + `Vec` 分配 | ghost 预览用扁平表 |
//! | `view_main` → 判空 | 全量扫描 + `NoteEvent` 全量拷贝 | 一个 `bool` |
//!
//! 命中集合是 `(document, arrange_selection, track_visual_order)` 的**纯函数**，
//! 修法是按三者版本号缓存（`selection_cache`）。
//!
//! # 本基准验证什么
//!
//! 模拟 view 层**每帧**的两类查询（扁平表 + 判空），对比：
//! - **旧口径**：每帧强制全量重算（绕过缓存，直接调扫描）
//! - **新口径**：每帧走缓存（键命中即 O(1)）
//!
//! 帧率线：60 FPS 预算 16.67ms/帧；本基准断言新口径单帧耗时 ≤ 1ms。
//!
//! 运行：
//! ```bash
//! cargo bench -p lumino-ui-editor --bench arrangement_sel_cache_bench
//! # 调整规模：LUMINO_BENCH_TRACKS / LUMINO_BENCH_NOTES_PER_TRACK
//! ```
//!
//! 环境变量：`LUMINO_BENCH_TRACKS`（默认 16）/
//! `LUMINO_BENCH_NOTES_PER_TRACK`（默认 20000，即 32W 音符）/
//! `LUMINO_BENCH_FRAMES`（默认 120）

use std::time::Instant;

use lumino_ui_editor::Editor;

const DEFAULT_TRACKS: usize = 16;
const DEFAULT_NOTES_PER_TRACK: usize = 20_000;
const DEFAULT_FRAMES: usize = 120;

/// 单帧耗时硬指标（ms）—— 60 FPS 预算 16.67ms，本线留足余量给真实 view 树
const FRAME_TARGET_MS: f64 = 1.0;

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
        .max(1)
}

/// 构造 N 轨 × M 音符的工程，走带选区框住**全部**音符（最坏情况：全命中）
fn build_editor(tracks: usize, notes_per_track: usize) -> Editor {
    let mut editor = Editor::new();
    let mut doc = lumino_midi_loader::MidiDocument {
        notes: (0..tracks)
            .map(|_| lumino_midi_loader::ChunkedList::new())
            .collect(),
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![],
        control_events: lumino_midi_loader::ChunkedList::new(),
        lyrics: vec![],
        markers: vec![],
        text_events: vec![],
        sys_ex: vec![],
        track_names: (0..tracks).map(|i| Some(format!("Track {i}"))).collect(),
        total_ticks: 0,
        track_count: tracks as u16,
        tracks: lumino_midi_loader::TrackManager::new(tracks as u16),
        division: 480,
        track_ports: vec![0; tracks],
        track_max_end_ticks: lumino_midi_loader::MidiDocument::new_track_max_ticks(tracks),
    };

    // 每轨 M 个音符，tick 升序、key 循环 0..127
    for t in 0..tracks {
        let events: Vec<lumino_midi_loader::NoteEvent> = (0..notes_per_track)
            .map(|i| {
                lumino_midi_loader::NoteEvent::new(
                    (i * 10) as u32,
                    (i * 10 + 8) as u32,
                    (i % 128) as u8,
                    100,
                    0,
                )
            })
            .collect();
        doc.notes[t] = lumino_midi_loader::ChunkedList::from_sorted(events);
    }
    doc.total_ticks = (notes_per_track * 10) as u32;

    editor.editor_state.data.document = Some(doc);
    editor.editor_state.data.current_track = 0;
    // 恒等视觉映射（侧边栏未排序时的常态）
    editor.editor_state.data.track_visual_order = (0..tracks).collect();

    // 选区框住全部音符（视觉轨 0..tracks-1，全 key，全 tick）
    editor.editor_state.data.arrange_selection.add_rect_track(
        0,
        u32::MAX,
        0,
        127,
        0,
        (tracks - 1) as u16,
    );
    editor
}

/// 中位数（帧耗时的判定口径：抗单帧抖动）
fn median(v: &mut [f64]) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).expect("耗时不应为 NaN"));
    v[v.len() / 2]
}

/// 微秒样本 → 毫秒样本（复用同一个中位数口径）
fn new_frame_ms_from(us: &[f64]) -> Vec<f64> {
    us.iter().map(|v| v / 1000.0).collect()
}

fn main() {
    let tracks = env_usize("LUMINO_BENCH_TRACKS", DEFAULT_TRACKS);
    let notes_per_track = env_usize("LUMINO_BENCH_NOTES_PER_TRACK", DEFAULT_NOTES_PER_TRACK);
    let frames = env_usize("LUMINO_BENCH_FRAMES", DEFAULT_FRAMES);
    let total_notes = tracks * notes_per_track;

    println!("=== 走带选区命中音符 · view 层每帧派生查询基准 ===");
    println!(
        "规模: {tracks} 轨 × {notes_per_track} 音符 = {total_notes} 音符（全命中选区）| 帧数 {frames}"
    );
    println!("单帧耗时线 ≤ {FRAME_TARGET_MS:.2} ms（60 FPS 预算 16.67ms）\n");

    let t_build = Instant::now();
    let mut editor = build_editor(tracks, notes_per_track);
    println!("工程构建: {:.2} s", t_build.elapsed().as_secs_f64());

    // 正确性前置：选区必须全命中
    let hit = editor.arrangement_selected_notes().len();
    assert_eq!(
        hit, total_notes,
        "选区应全命中 {total_notes} 个音符，实际 {hit} —— 基准数据不自洽"
    );
    assert!(editor.arrangement_has_selected_notes());
    println!("校验: 选区命中 {hit} 音符 ✓\n");

    // ── 新口径：每帧走缓存（生产路径）──
    // 缓存命中耗时低于 `Instant` 单次计时分辨率（亚微秒），故测**整轮总时长**
    // 再求每帧均值，得到可解读的数字；峰值另用单帧计时给出上界。
    let mut new_frame_us: Vec<f64> = Vec::with_capacity(frames);
    let t_new_total = Instant::now();
    for _ in 0..frames {
        let t = Instant::now();
        // 模拟 view_arrangement：取 ghost 预览用扁平表
        let notes = editor.arrangement_selected_notes();
        // 模拟 view_main：菜单可用性判空（零分配）
        let enabled = editor.arrangement_has_selected_notes();
        let elapsed_us = t.elapsed().as_secs_f64() * 1e6;
        std::hint::black_box((notes.len(), enabled));
        new_frame_us.push(elapsed_us);
    }
    let new_total_ms = t_new_total.elapsed().as_secs_f64() * 1000.0;

    // ── 旧口径对照：每帧强制全量重算 ──
    // 通过 bump 选区版本号使缓存必然失效，模拟「无缓存的每帧全量扫描」
    let mut old_frame_ms: Vec<f64> = Vec::with_capacity(frames);
    for _ in 0..frames {
        let t = Instant::now();
        // 选区平移 0 tick 也会 bump revision → 强制重扫（几何不变，结果等价）
        editor.editor_state.data.arrange_selection.offset_ticks(0);
        let notes = editor.arrangement_selected_notes();
        let _enabled = editor.arrangement_has_selected_notes();
        std::hint::black_box(notes.len());
        old_frame_ms.push(t.elapsed().as_secs_f64() * 1000.0);
    }

    let new_avg_us = new_total_ms * 1000.0 / frames as f64;
    let mut new_frame_ms = new_frame_ms_from(&new_frame_us);
    let new_median_ms = median(&mut new_frame_ms);
    let old_median = median(&mut old_frame_ms);
    let old_max = old_frame_ms.iter().copied().fold(0.0f64, f64::max);
    let speedup = if new_avg_us > 0.0 {
        old_median * 1000.0 / new_avg_us
    } else {
        f64::INFINITY
    };

    let new_median_us = new_median_ms * 1000.0;
    println!("── view 层单帧耗时（两类查询合计，{frames} 帧）──");
    println!("  旧口径（每帧全量重算）: 中位 {old_median:8.3} ms | 峰值 {old_max:8.3} ms");
    println!(
        "  新口径（缓存命中）    : 均值 {new_avg_us:8.3} µs | 中位 {new_median_us:8.3} µs \
（整轮总时长 {new_total_ms:.3} ms）"
    );
    println!("  加速比: {speedup:.0}x（缓存命中耗时已接近计时器噪声下限）\n");

    // 60 FPS 预算对照
    let fps_old = 1000.0 / old_median.max(f64::MIN_POSITIVE);
    println!("── 预算对照 ──");
    println!(
        "  旧口径单这两类查询就吃掉 {old_median:.2} ms/帧 → 折算 {fps_old:.0} FPS 上限\
（尚未计入其余 view 树与 GPU 提交）"
    );
    println!("  新口径 {new_avg_us:.3} µs/帧 → 该项不再是帧时间瓶颈\n");

    let pass = new_avg_us / 1000.0 <= FRAME_TARGET_MS && old_median > FRAME_TARGET_MS;
    println!("结论: {}", if pass { "✅ PASS" } else { "❌ FAIL" });
    if pass {
        println!(
            "  新口径 {new_avg_us:.3} µs/帧（线 {FRAME_TARGET_MS:.2} ms）；\
旧口径 {old_median:.2} ms/帧 已超线，修复后回到预算内"
        );
    } else {
        println!("  新口径 {new_avg_us:.3} µs/帧 超线 {FRAME_TARGET_MS:.2} ms");
    }
    println!("=== 完成 ===");

    if !pass {
        std::process::exit(1);
    }
}
