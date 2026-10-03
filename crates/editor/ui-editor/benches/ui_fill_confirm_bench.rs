//! 曲线工具 + 颜料桶填充：一次性生成 >1 万音符的性能基准（√ 确认）
//!
//! 复现背景（用户报告）：**曲线工具划定封闭图形 + 颜料桶填充，√ 确认瞬间填充
//! 超过 1 万个音符时卷帘 WGPU 卡顿**。
//!
//! 根因假设（本基准量化验证）：
//! `confirm_line_tool` 对生成的全部音符**逐个** `insert_note_with_id` 写入 document；
//! 每次当前轨插入都记一条 `NoteDeltaEvent::InsertAt`，渲染侧
//! （`note_update::update_note_data_for_wgpu_thread`）对**每条**事件发**一条**
//! `NoteEvent::Insert` 消息；渲染线程每条消息执行一次
//! `GpuNoteBuffer::insert_at` → `move_range`（把该索引之后的**全部**实例在 GPU 内
//! 搬移一次 + 新建 staging buffer + 一次 queue.submit）。
//!
//! `fill_spans` 的输出顺序是「按音高行、行内按 tick」——**跨行 tick 回绕**，
//! 于是每条插入都落在列表中部，`Σtail` 随 N **超线性**增长（本负载实测：
//! 音符数翻倍 → Σtail ×≈3.0–3.3，介于 O(N) 的 2× 与 O(N²) 的 4× 之间；
//! 插入序列非随机，故低于理论 N²/2 上界）。
//! 万级音符 = 数千万次实例搬移（数百 MB GPU 拷贝）+ 上万次 submit，
//! 这就是卡顿的来源。
//!
//! 基准内容（三路径对照，修完后仍可回归）：
//! - **P 生产路径**：真实链路 `EditorAction::LineToolConfirm`；
//! - **L 逐音符对照**：复刻历史实现（`insert_note_with_id` 循环），
//!   永久记录该悬崖；若生产回归到逐音符，P 会重新劣化到 L 的水平；
//! - **B 批量对照**：同一音符载荷走 `batch_insert_notes_with_ids`
//!   （单次归并 + 一次 `TrackDelta` 结构重建，`note_delta_events` 清空）；
//! - **渲染侧代价模型**（无 GPU 环境下的诚实代理）：按事件逐条累加
//!   `GPU 消息数 / 搬移实例数 / move 提交数 / write_buffer 次数`；
//! - **增长曲线**：图形宽度扫描 → 生产路径的 Σtail 增长阶数。
//!
//! 运行：`cargo bench -p lumino-ui-editor --bench ui_fill_confirm_bench`
//! 环境变量：`LUMINO_FILL_BENCH_WIDTH` / `LUMINO_FILL_BENCH_DIVISION`
//! / `LUMINO_FILL_BENCH_VERBOSE`。

use std::env;
use std::time::Instant;

use lumino_editor_state::{BezierAnchor, LinePath, NoteDeltaEvent};
use lumino_midi_loader::{MidiDocument, NoteEvent};
use lumino_ui_editor::message::EditorAction;
use lumino_ui_editor::{Editor, Note};

/// 当前轨（track 0 = Conductor，曲线工具禁用 → 用 1）
const TRACK: usize = 1;
/// PPQ（与 `MidiDocument::empty_with_tracks` 一致）
const PPQ: u16 = 960;
/// 填充图形宽度（tick）；默认 ≈ 生成 1.3 万音符
const DEFAULT_WIDTH: f32 = 36000.0;
/// 填充切分档位（x 分音符）；`None` = 整块填充
const DEFAULT_DIVISION: u32 = 16;
/// 生产路径 √ 确认耗时硬指标（ms）——单次用户手势，须在 1 帧内完成
const TARGET_CONFIRM_MS: f64 = 50.0;
/// 图形 tick 起点
const T0: f32 = 480.0;
/// 图形音高下界 / 上界（88 行）
const K_LO: u16 = 40;
const K_HI: u16 = 127;
/// 既有工程底噪音符数（不落在填充音高区间，便于提取填充载荷）
const BASELINE: usize = 2000;
/// 增长曲线的图形宽度序列（tick）
const GROWTH_WIDTHS: [f32; 5] = [6000.0, 12000.0, 24000.0, 48000.0, 96000.0];

/// 渲染侧代价模型（`GpuNoteBuffer::insert_at` 的逐步展开）
#[derive(Default, Clone, Copy)]
struct RenderCost {
    /// 发往渲染线程的消息条数
    msgs: u64,
    /// GPU 内被搬移的实例总数（Σtail）——真实 GPU 拷贝量的直接代理
    moved_instances: u64,
    /// `move_range` 调用次数（每次含 staging buffer 创建 + submit）
    move_calls: u64,
    /// `write_buffer` 次数
    writes: u64,
}

impl RenderCost {
    /// 搬移字节数（NoteInstance = 16 字节）
    fn moved_mb(&self) -> f64 {
        self.moved_instances as f64 * 16.0 / (1024.0 * 1024.0)
    }
}

/// 按渲染侧事件处理逻辑展开代价模型
///
/// 与 `crates/editor/ui/src/host/render/separate_thread/runner/note_update.rs`
/// 的 `NoteDeltaEvent` 分支 + `gpu_note_buffer::insert_at` 保持同构。
fn model_render_cost(events: &[NoteDeltaEvent], initial_count: usize) -> RenderCost {
    let mut c = RenderCost::default();
    let mut count = initial_count;
    for ev in events {
        match ev {
            NoteDeltaEvent::InsertAt { index, .. } => {
                c.msgs += 1;
                c.writes += 1;
                let tail = count.saturating_sub(*index) as u64;
                if tail > 0 {
                    c.moved_instances += tail;
                    c.move_calls += 1;
                }
                count += 1;
            }
            NoteDeltaEvent::UpdateRange { .. } => {
                c.msgs += 1;
                c.writes += 1;
            }
            NoteDeltaEvent::RemoveAt { .. } => {
                c.msgs += 1;
            }
        }
    }
    c
}

/// 装配编辑器：2 轨空文档 + 底噪音符（key 20，避开填充音高区间）
fn build_editor() -> Editor {
    let mut doc = MidiDocument::empty_with_tracks(2, PPQ);
    let base: Vec<NoteEvent> = (0..BASELINE as u32)
        .map(|i| NoteEvent::new(i * 100, i * 100 + 60, 20, 100, 0))
        .collect();
    doc.batch_insert_sorted_notes_with_ids(TRACK, base);

    let mut editor = Editor::new();
    editor.editor_state.data.document = Some(doc);
    editor.editor_state.data.current_track = TRACK;
    editor.editor_state.data.set_collab_sync_enabled(false);
    // 视图：像素/键与像素/tick 取 1 比值量，坐标换算不引入额外误差
    let v = &mut editor.editor_state.view;
    v.ppq = PPQ;
    v.key_count = 128;
    v.visible_key_count = 128;
    v.snap_precision = 240.0;
    v.zoom_x = 0.05;
    v.zoom_y = 8.0;
    v.scroll_x = 0.0;
    v.scroll_y = 0.0;
    editor.editor_state.canvas.size_x = 8000.0;
    editor.editor_state.canvas.size_y = 1200.0;
    editor
}

/// 装载「封闭矩形 + 单点填充标记」的曲线工具状态（√ 前的状态）
fn arm_fill(editor: &mut Editor, width: f32, division: Option<u32>) {
    let t1 = T0 + width;
    let rect: LinePath = [(T0, K_LO), (t1, K_LO), (t1, K_HI), (T0, K_HI), (T0, K_LO)]
        .into_iter()
        .map(|(t, k)| BezierAnchor::new((t, k as f32)))
        .collect();
    let lt = &mut editor.editor_state.line_tool;
    lt.paths = vec![rect];
    lt.fill = vec![(T0 + width / 2.0, 60)];
    lt.fill_division = division;
    lt.fill_enabled = true;
}

/// 从当前轨取出「填充载荷」（key ≥ K_LO 的音符，升序），供 B 路径复用同一载荷
fn extract_payload(editor: &Editor) -> Vec<NoteEvent> {
    editor
        .editor_state
        .data
        .current_track_notes()
        .iter()
        .filter(|n| n.key >= K_LO as u8)
        .copied()
        .collect()
}

/// 单档结果（三路径）
struct Row {
    width: f32,
    notes: usize,
    /// P：生产路径（`EditorAction::LineToolConfirm`）
    p_ms: f64,
    p_cost: RenderCost,
    p_events: usize,
    /// L：逐音符对照（历史实现复刻）
    l_ms: f64,
    l_cost: RenderCost,
    l_events: usize,
    /// B：批量归并对照
    b_ms: f64,
    b_cost: RenderCost,
    b_events: usize,
    b_struct_dirty: bool,
}

/// 复刻历史实现：逐音符 `insert_note_with_id` 循环（修复前生产的写法）
///
/// 插入顺序按 `(key, tick)` 复刻 `fill_spans` 的「按音高行、行内按 tick」生成序
/// ——这正是让每条插入都落在列表中部、`Σtail` 超线性的关键。若改按 tick 升序
/// 插入（尾部搬移趋零），会显著低估历史实现的代价。
fn write_per_note(editor: &mut Editor, payload: &[NoteEvent]) -> usize {
    let mut ordered: Vec<&NoteEvent> = payload.iter().collect();
    ordered.sort_by_key(|e| (e.key, e.start_tick));
    let mut n = 0usize;
    for ev in ordered {
        let note = Note::new(
            ev.start_tick as f32,
            ev.key as u16,
            (ev.end_tick - ev.start_tick) as f32,
        );
        if editor
            .editor_state
            .data
            .insert_note_with_id(TRACK, note)
            .is_some()
        {
            n += 1;
        }
    }
    n
}

/// 跑一档：P 生产链路 / L 逐音符对照 / B 批量对照（同一音符载荷）
fn run_row(width: f32, division: Option<u32>) -> Row {
    // ── P：真实生产链路 ──
    let mut editor = build_editor();
    let base_count = editor.editor_state.data.current_track_note_count();
    arm_fill(&mut editor, width, division);

    let t0 = Instant::now();
    editor.handle_action(EditorAction::LineToolConfirm);
    let p_ms = t0.elapsed().as_secs_f64() * 1000.0;

    let p_events_vec = editor.editor_state.data.note_delta_events.clone();
    let p_events = p_events_vec.len();
    let p_cost = model_render_cost(&p_events_vec, base_count);
    let payload = extract_payload(&editor);
    let notes = payload.len();

    // ── L：逐音符对照（同一载荷）──
    let mut editor_l = build_editor();
    let base_l = editor_l.editor_state.data.current_track_note_count();
    let tl = Instant::now();
    write_per_note(&mut editor_l, &payload);
    let l_ms = tl.elapsed().as_secs_f64() * 1000.0;
    let l_events_vec = editor_l.editor_state.data.note_delta_events.clone();
    let l_events = l_events_vec.len();
    let l_cost = model_render_cost(&l_events_vec, base_l);

    // ── B：同一载荷，批量归并写入 ──
    let mut editor_b = build_editor();
    let base_b = editor_b.editor_state.data.current_track_note_count();
    let payload_notes: Vec<Note> = payload
        .iter()
        .map(|ev| {
            Note::new(
                ev.start_tick as f32,
                ev.key as u16,
                (ev.end_tick - ev.start_tick) as f32,
            )
        })
        .collect();
    let t1 = Instant::now();
    editor_b
        .editor_state
        .data
        .batch_insert_notes_with_ids(&payload_notes);
    let b_ms = t1.elapsed().as_secs_f64() * 1000.0;
    let b_events_vec = editor_b.editor_state.data.note_delta_events.clone();
    let b_events = b_events_vec.len();
    let b_cost = model_render_cost(&b_events_vec, base_b);
    let b_struct_dirty = editor_b.editor_state.data.main_track_struct_dirty;

    Row {
        width,
        notes,
        p_ms,
        p_cost,
        p_events,
        l_ms,
        l_cost,
        l_events,
        b_ms,
        b_cost,
        b_events,
        b_struct_dirty,
    }
}

fn median(v: &[f64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    s[s.len() / 2]
}

fn env_f32(key: &str, default: f32) -> f32 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(default)
}

fn main() {
    println!("=== Lumino 基准：曲线工具 + 颜料桶填充 → √ 确认（>1 万音符）===");
    let width = env_f32("LUMINO_FILL_BENCH_WIDTH", DEFAULT_WIDTH);
    let division = Some(DEFAULT_DIVISION);
    let cycles = 5usize;

    let notes_hint = (width / (4.0 * PPQ as f32 / DEFAULT_DIVISION as f32))
        * (K_HI - K_LO + 1) as f32;
    println!(
        "工作负载: 矩形宽 {width:.0} tick × {} 行 | 切分 1/{DEFAULT_DIVISION} | 预估音符 ≈ {notes_hint:.0}",
        K_HI - K_LO + 1
    );
    println!(
        "路径 P = 生产 √ 确认（`EditorAction::LineToolConfirm`）| \
L = 逐音符对照（历史实现）| B = 同载荷批量归并"
    );
    println!();

    // 暖机
    let _ = run_row(width, division);

    let (mut p_ms, mut l_ms, mut b_ms) = (Vec::new(), Vec::new(), Vec::new());
    let mut last = run_row(width, division);
    for _ in 0..cycles {
        let r = run_row(width, division);
        p_ms.push(r.p_ms);
        l_ms.push(r.l_ms);
        b_ms.push(r.b_ms);
        last = r;
    }

    println!("── 单档（{} 音符）──", last.notes);
    println!(
        "{:<30} | {:>13} | {:>13} | {:>11}",
        "指标", "P 生产路径", "L 逐音符对照", "B 批量"
    );
    println!("{}", "-".repeat(76));
    let row = |name: &str, p: String, l: String, b: String| {
        println!("{:<30} | {:>13} | {:>13} | {:>11}", name, p, l, b);
    };
    row(
        "写入耗时（中位）",
        format!("{:.1} ms", median(&p_ms)),
        format!("{:.1} ms", median(&l_ms)),
        format!("{:.1} ms", median(&b_ms)),
    );
    row(
        "delta 事件数",
        format!("{}", last.p_events),
        format!("{}", last.l_events),
        format!("{}", last.b_events),
    );
    row(
        "GPU 消息数",
        format!("{}", last.p_cost.msgs),
        format!("{}", last.l_cost.msgs),
        format!("{}", last.b_cost.msgs),
    );
    row(
        "GPU 搬移实例数 Σtail",
        format!("{}", last.p_cost.moved_instances),
        format!("{}", last.l_cost.moved_instances),
        format!("{}", last.b_cost.moved_instances),
    );
    row(
        "GPU 拷贝量（≈）",
        format!("{:.1} MB", last.p_cost.moved_mb()),
        format!("{:.1} MB", last.l_cost.moved_mb()),
        format!("{:.1} MB", last.b_cost.moved_mb()),
    );
    row(
        "move_range/submit 次数",
        format!("{}", last.p_cost.move_calls),
        format!("{}", last.l_cost.move_calls),
        format!("{}", last.b_cost.move_calls),
    );
    row(
        "write_buffer 次数",
        format!("{}", last.p_cost.writes),
        format!("{}", last.l_cost.writes),
        format!("{}", last.b_cost.writes),
    );
    row(
        "结构重建（TrackDelta）",
        if last.p_cost.msgs == 0 { "是（1 次）" } else { "否" }.into(),
        if last.l_cost.msgs == 0 { "是" } else { "否" }.into(),
        if last.b_struct_dirty { "是（1 次）" } else { "否" }.into(),
    );

    let p_med = median(&p_ms);
    let b_med = median(&b_ms);
    let gate_time = p_med <= TARGET_CONFIRM_MS;
    let gate_render = last.p_cost.moved_instances == 0;
    println!();
    println!(
        "判定：生产路径 √ 确认 {p_med:.1} ms（线 ≤ {TARGET_CONFIRM_MS:.0} ms）{} | \
GPU 搬移实例 {}（要求 0）{} | 相对批量路径 {:.1}×",
        if gate_time { "✓" } else { "✗ 超标" },
        last.p_cost.moved_instances,
        if gate_render { "✓" } else { "✗ 未消除" },
        if p_med > 0.0 && b_med > 0.0 {
            p_med / b_med
        } else {
            f64::NAN
        }
    );
    let l_med = median(&l_ms);
    if l_med > 0.0 && b_med > 0.0 {
        println!(
            "对照：逐音符路径 {l_med:.1} ms / 搬移 {:.0} 万实例 → 批量 {b_med:.1} ms / 搬移 0（提速 {:.0}×）",
            last.l_cost.moved_instances as f64 / 1e4,
            l_med / b_med
        );
    }

    // ── 增长曲线：生产路径 Σtail 随 N 的增长阶数 ──
    println!();
    println!("── 增长曲线（图形宽度扫描）──");
    println!(
        "{:>9} | {:>9} | {:>11} | {:>11} | {:>13} | {:>10}",
        "宽度", "音符数", "P 耗时 ms", "P Σtail(万)", "翻倍→Σtail 增长", "B 耗时 ms"
    );
    println!("{}", "-".repeat(78));
    let mut prev: Option<(usize, u64)> = None;
    for w in GROWTH_WIDTHS {
        let r = run_row(w, division);
        let growth = prev.map_or(String::from("—"), |(pn, pm)| {
            format!(
                "{:.2}× (N {:.2}×)",
                r.p_cost.moved_instances as f64 / pm.max(1) as f64,
                r.notes as f64 / pn.max(1) as f64
            )
        });
        println!(
            "{:>9.0} | {:>9} | {:>11.1} | {:>11.1} | {:>13} | {:>10.1}",
            r.width,
            r.notes,
            r.p_ms,
            r.p_cost.moved_instances as f64 / 1e4,
            growth,
            r.b_ms
        );
        prev = Some((r.notes, r.p_cost.moved_instances));
    }
}
