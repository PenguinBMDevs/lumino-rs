//! RenderParams 构建辅助：全量收集、排序打包、诊断打点与二分窗口
//!
//! 自 `render_params.rs` 逐字节搬移。

use lumino_extras::palette::current_track_color_f32;
use lumino_gfx::{NoteInstance, pack_key_color};
use lumino_midi_loader::{MidiDocument, NoteEvent};

use super::SortableNote;

/// 首帧全量收集：全文档音符（无窗口过滤，窗口过滤走 GPU cull）。
///
/// 与窗口收集的区别仅在过滤条件：此处不过滤 `end_tick/start_tick`，只做 key 范围
/// 过滤（u16 比较；窗口路径的 `key_count as u8` 在 key_count=256 时回绕的上游
/// quirk 此处不继承）。输出经调用方排序 + 打包后即导出常驻内容（桶建其上）。
/// 调用方须保证同一导出任务首帧调用一次（`collect_all`），后续帧跳过。
pub(crate) fn collect_all_notes(
    document: &MidiDocument,
    key_count: u16,
    visible_notes: &mut Vec<SortableNote>,
) {
    visible_notes.clear();
    for (track_idx, track_notes) in document.notes.iter().enumerate() {
        let track_idx = track_idx as u16;
        for n in track_notes.iter() {
            if (n.key as u16) < key_count {
                visible_notes.push(SortableNote {
                    key: n.key,
                    start_tick: n.start_tick,
                    length: n.end_tick.saturating_sub(n.start_tick),
                    track_idx,
                });
            }
        }
    }
}

// 注：滑动窗口收集（`collect_window_notes` + 游标）已删除——GPU cull 接管窗口过滤
//（见 `bucket_cull.wgsl` 头注），逐帧 CPU 收集是导出主瓶颈；`note_search_bounds`
// 保留供流式重做复用。删除内容 git 历史可查。

/// 首帧收集分段打点（+ 每 300 帧）：用数据验证收集/排序/打包耗时，
/// 替代"感觉慢"的体感归因。输出示例：
/// `waterfall收集打点: collect=120us sort=3400us pack=900us visible=262713`。
pub(crate) fn diag_window_collect(
    mode: &'static str,
    collect_us: u64,
    sort_us: u64,
    pack_us: u64,
    visible: usize,
) {
    static DIAG_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = DIAG_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if n < 3 || n.is_multiple_of(300) {
        tracing::info!(
            "{mode}收集打点[{n}]: collect={collect_us}us sort={sort_us}us pack={pack_us}us visible={visible}"
        );
    }
}
/// （单帧 10W+ 音符）排序是每帧 CPU 热点，key 范围固定时用计数分桶省去 log 因子。
/// 桶内按 (start_tick, track 倒序) 稳定排序，与原 (key, start_tick, u16::MAX - track_idx)
/// 排序键去掉 key 维度后等价。三种 GPU 模式共用，保持派生换算输入顺序一致。
///
/// `scratch` 为调用方常驻暂存（`WindowCollectState.sort_scratch`），消每帧
/// V×`SortableNote` 整块分配；返回前内容无意义，调用方不得依赖。
pub(crate) fn sort_visible_notes(
    visible_notes: &mut Vec<SortableNote>,
    scratch: &mut Vec<SortableNote>,
) {
    const KEY_BUCKETS: usize = 256;
    let mut counts = [0u32; KEY_BUCKETS];
    for n in visible_notes.iter() {
        counts[n.key as usize] += 1;
    }
    let mut offsets = [0u32; KEY_BUCKETS + 1];
    for k in 0..KEY_BUCKETS {
        offsets[k + 1] = offsets[k] + counts[k];
    }
    scratch.clear();
    scratch.reserve(visible_notes.len());
    scratch.extend(visible_notes.iter().cloned());
    let sorted_notes = scratch;
    let mut cursor = offsets[..KEY_BUCKETS].to_vec();
    for n in visible_notes.iter() {
        let k = n.key as usize;
        sorted_notes[cursor[k] as usize] = n.clone();
        cursor[k] += 1;
    }
    let mut seg_start = 0usize;
    for k in 0..KEY_BUCKETS {
        let seg_end = offsets[k + 1] as usize;
        sorted_notes[seg_start..seg_end].sort_by_key(|n| (n.start_tick, u16::MAX - n.track_idx));
        seg_start = seg_end;
    }
    std::mem::swap(visible_notes, sorted_notes);
}

/// 将已排序可见音符打包为 `NoteInstance`（wasabi 风格 border_width 由调用方按视口算好传入）。
/// 瀑布流 / 3D 模式传入 0 即可——渲染线程换算时只读 key/start/length/color，忽略边框。
pub(crate) fn pack_note_instances(
    visible_notes: &[SortableNote],
    border_width: u32,
    note_instances_out: &mut Vec<NoteInstance>,
) {
    note_instances_out.clear();
    note_instances_out.reserve(visible_notes.len());
    for n in visible_notes.iter() {
        let key_color = pack_key_color(n.key, current_track_color_f32(n.track_idx as usize));
        note_instances_out.push(NoteInstance {
            start_length: [n.start_tick as f32, (n.length as f32).max(1.0)],
            key_color,
            border_width,
        });
    }
}

/// 计算音符数组的二分搜索窗口 `[start, end)`（半开区间）
///
/// 保留供流式重做使用（全量常驻模式暂不需要逐帧窗口）：
/// `MidiDocument.notes` 每轨按 `start_tick` 升序排列（见 document.rs）。
/// 视口 `[tick_start, tick_end]` 内的可见音符必然满足：
/// - `start_tick <= tick_end`（音符必须已开始）；
/// - 任意时长的跨视口长音符（即使 `start_tick` 远早于 `tick_start`）只要
///   `end_tick >= tick_start` 即为可见——因此下界固定为 0，不使用固定
///   `TICK_SEARCH_BUFFER`，否则时长超过该缓冲区的超长音符在半路消失。
///   （见：`build_note_rectangle_render_params` 各模式收集逻辑）
///
/// 上界仍通过二分查找定位，避免扫描文件末尾的未开始音符。
/// `pub(crate)`：供 `waterfall_frame.rs`（CPU 瀑布流）与各 GPU 模式收集复用同一窗口逻辑。
#[allow(dead_code)] // 内存路径已切滑动窗口收集；流式重做时复用，测试仍覆盖
pub(crate) fn note_search_bounds(
    track_notes: &lumino_midi_loader::ChunkedList<NoteEvent>,
    _tick_start: u32,
    tick_end: u32,
) -> (usize, usize) {
    // 下界固定为 0：超长音符的 start_tick 可能远早于 tick_start - TICK_SEARCH_BUFFER，
    // 但 end_tick 仍在当前 tick 之后，必须被纳入搜索窗口。
    // 上界：第一个 start_tick > tick_end 的索引（等价于旧 partition_point(|n| n.start_tick <= tick_end)）
    let search_end = track_notes.partition_point(tick_end.wrapping_add(1));
    (0, search_end)
}
