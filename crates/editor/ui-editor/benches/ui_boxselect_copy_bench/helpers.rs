//! UI 路径基准支撑：单轮执行、统计报告、渲染侧实例构建代理
//!
//! 由 `ui_boxselect_copy_bench.rs` 拆出（保持单文件 < 400 行）。

use std::env;

use lumino_ui_editor::message::{EditorAction, Point2, Tool};
use lumino_ui_editor::{EditState, Editor};

use crate::support::{Stat, mb};

/// 单次操作内存上升硬指标（MB）
pub const MEM_TARGET_MB: f64 = 150.0;
/// 粘贴（追加锚点·快路径）单次内存上升硬指标（MB）
///
/// 280W 事件解码（68MB）+ 归并输出块（68MB）+ 剪贴板载荷（20MB）≈ 160MB 固有，
/// 故粘贴内存线独立放宽；重叠锚点归并路径瞬时更高（信息性报道）。
pub const PASTE_MEM_TARGET_MB: f64 = 200.0;

/// 读取浮点环境变量（缺失/非法时回退默认值）
pub fn env_f64(key: &str, default: f64) -> f64 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(default)
}

/// 读取布尔环境变量（`0` 为 false，其余为 true）
pub fn env_bool(key: &str, default: bool) -> bool {
    env::var(key).ok().map(|v| v != "0").unwrap_or(default)
}

/// 单轮测量统计
pub struct CycleStats {
    pub moves: Stat,
    pub released: Stat,
    pub binary: Stat,
    pub full_copy: Stat,
    pub selected: usize,
    pub payload_len: usize,
}

impl CycleStats {
    pub fn new() -> Self {
        Self {
            moves: Stat::new("框选（单次 Moved）"),
            released: Stat::new("释放（Released）"),
            binary: Stat::new("复制（二进制编码）"),
            full_copy: Stat::new("复制（handle_action 全路径）"),
            selected: 0,
            payload_len: 0,
        }
    }
}

impl Default for CycleStats {
    fn default() -> Self {
        Self::new()
    }
}

/// 工作负载与运行参数（打包传递，避免 `run_cycle` 参数过多）
pub struct Workload {
    pub x0: f32,
    pub sel_end: f32,
    pub moves: usize,
    pub track: usize,
    pub division: u16,
    pub do_clipboard: bool,
}

/// 执行一轮：框选（Pressed → N×Moved → Released）→ 二进制编码 → 全路径复制 → 复位。
///
/// 返回数据校验是否通过（选中数与载荷均非空）。
pub fn run_cycle(editor: &mut Editor, stats: &mut CycleStats, w: &Workload) -> bool {
    // 起点在首个音符之前（无音符处）→ 必然进入框选而非音符编辑
    editor.handle_action(EditorAction::Pressed {
        pos: Point2::new(w.x0, 127.0),
        shift: false,
        ctrl: false,
    });
    if !matches!(
        editor.editor_state.interaction.edit_state,
        EditState::Selecting { .. }
    ) {
        eprintln!("✗ 未进入框选状态（按下命中音符？）");
        return false;
    }

    for i in 1..=w.moves {
        let f = i as f32 / w.moves as f32;
        let x = w.x0 + (w.sel_end - w.x0) * f;
        let y = 127.0 * (1.0 - f);
        stats
            .moves
            .measure(|| editor.handle_action(EditorAction::Moved(Point2::new(x, y))));
    }

    stats
        .released
        .measure(|| editor.handle_action(EditorAction::Released));

    let payload = stats
        .binary
        .measure(|| editor.build_clipboard_binary(w.track, w.division));
    let payload_len = payload.as_ref().map(|b| b.len()).unwrap_or(0);
    drop(payload);

    if w.do_clipboard {
        stats
            .full_copy
            .measure(|| editor.handle_action(EditorAction::Copy));
    }

    stats.selected = editor.selected_notes_count();
    stats.payload_len = payload_len;

    // 复位：清空选中（下一轮从零开始框选）
    editor.clear_selection();
    editor.editor_state.interaction.edit_state = EditState::Idle;

    stats.selected > 0 && payload_len > 0
}

/// 输出单操作统计（中位/最小/最大 + 内存峰值），返回 (耗时达标, 内存达标)。
///
/// `mem_target_mb=None` 时内存仅信息性报道（不参与判定）——用于渲染侧实例缓冲等
/// 固有大数据结构。
pub fn report_op(
    name: &str,
    times: &[f64],
    target_ms: f64,
    peak_growth_bytes: i64,
    mem_target_mb: Option<f64>,
) -> (bool, bool) {
    if times.is_empty() {
        println!("{name:<28} | （无样本）");
        return (true, true);
    }
    let median = median(times);
    let max = times.iter().copied().fold(0.0f64, f64::max);
    let min = times.iter().copied().fold(f64::INFINITY, f64::min);
    let time_pass = median <= target_ms;
    let mem_mb = mb(peak_growth_bytes);
    let mem_pass = mem_target_mb.is_none_or(|target| mem_mb <= target);
    if env::var_os("LUMINO_UI_BENCH_VERBOSE").is_some() {
        let samples: Vec<String> = times.iter().map(|t| format!("{t:.1}")).collect();
        println!("  [明细] {name}: {} ms", samples.join(", "));
    }
    let mem_label = if mem_target_mb.is_none() {
        format!("内存峰值 {mem_mb:>7.1} MB（信息）")
    } else if mem_pass {
        format!("内存峰值 {mem_mb:>7.1} MB")
    } else {
        format!("内存峰值 {mem_mb:>7.1} MB ✗ 超标")
    };
    println!(
        "{name:<28} | 中位 {median:>8.2} ms | 最小 {min:>8.2} ms | 最大 {max:>8.2} ms | {mem_label} | {}",
        if time_pass {
            "✓ 达标"
        } else {
            "✗ 耗时超标"
        }
    );
    (time_pass, mem_pass)
}

/// 中位数（共享开发机单轮最大受调度噪声支配）
pub fn median(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    let mut s = xs.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    s[s.len() / 2]
}

/// 构建单轨完整 `NoteInstance` 分片列表（与渲染侧 `build_track_instance_parts` 同构）。
///
/// 渲染侧主轨段重建（`TrackDelta`）的 CPU 成本代理：大粘贴后当前轨
/// ~3000W 音符的实例构建（并行分片，免二次拼接拷贝）。
pub fn build_track_instances_proxy(
    editor: &Editor,
    track: usize,
) -> Vec<Vec<lumino_gfx::NoteInstance>> {
    const PARALLEL_BUILD_MIN: usize = 1_000_000;
    let color = [0.5f32, 0.5, 0.5, 1.0];
    let Some(doc) = editor.editor_state.data.document.as_ref() else {
        return Vec::new();
    };
    let doc_notes = doc.track_notes(track);
    let total = doc_notes.len();
    if total == 0 {
        return Vec::new();
    }
    if total < PARALLEL_BUILD_MIN {
        let mut v = Vec::with_capacity(total);
        for ne in doc_notes.iter() {
            v.push(lumino_gfx::NoteInstance::new(
                ne.start_tick as f32,
                ne.key,
                (ne.end_tick - ne.start_tick) as f32,
                color,
                1,
            ));
        }
        return vec![v];
    }
    use rayon::prelude::*;
    let workers = rayon::current_num_threads().max(1);
    let part = total.div_ceil(workers);
    (0..workers)
        .into_par_iter()
        .map(|w| {
            let lo = (w * part).min(total);
            let hi = ((w + 1) * part).min(total);
            // 精确预分配（iter_window 无 size_hint，避免倍增重分配）
            let mut v = Vec::with_capacity(hi.saturating_sub(lo));
            for (_, ne) in doc_notes.iter_window(lo, hi) {
                v.push(lumino_gfx::NoteInstance::new(
                    ne.start_tick as f32,
                    ne.key,
                    (ne.end_tick - ne.start_tick) as f32,
                    color,
                    1,
                ));
            }
            v
        })
        .collect()
}

/// 供 `main` 使用的编辑器装配辅助（真实坐标映射：zoom=1/scroll=0 → x=tick、y=127-key）
pub fn configure_editor(editor: &mut Editor) {
    editor.editor_state.tool = Tool::Pointer;
    let v = &mut editor.editor_state.view;
    v.visible_key_count = 128;
    v.zoom_x = 1.0;
    v.scroll_x = 0.0;
    v.keyboard_width = 0.0;
    v.zoom_y = 1.0;
    v.scroll_y = 0.0;
    v.ruler_height = 0.0;
    editor.editor_state.canvas.size_x = 4_000_000.0;
    editor.editor_state.canvas.size_y = 200.0;
}
