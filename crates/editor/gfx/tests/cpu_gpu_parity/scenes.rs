//! 对比用例（同一份 `MidiDocument` 供 CPU / GPU 两路渲染）。
//!
//! 画布 256×144、键数 41（`max_key_index = 40`）、`zoom_y = 10 px/key`：
//! `y = (40 - key) * 10`，可见键 25..40。所有用例的音符都落在视口内，
//! 且坐标在 100% 缩放下为整数（GPU 光栅与 CPU 像素中心规则逐位可比）。

use lumino_midi_model::{MidiDocument, NoteEvent};

/// 键总数（`max_key_index = 40`，与 CPU `key_count` / GPU `max_key_index` 同源）。
pub const KEY_COUNT: u16 = 41;
/// 默认纵向缩放（像素/键）。
pub const ZOOM_Y: f32 = 10.0;

/// 一个对比用例。
pub struct Case {
    /// 用例名（打印到报告里）。
    pub label: &'static str,
    /// 唯一权威音符源。
    pub document: MidiDocument,
    /// CPU 侧「当前轨」（该轨不画；GPU 侧对应轨道不进上传集合）。
    pub current_track: usize,
    /// CPU 侧静音位图（GPU 侧对应轨道不进上传集合）。
    pub muted: Vec<bool>,
    /// 相机缩放 `[zoom_x, zoom_y]`。
    pub zoom: [f32; 2],
    /// 相机滚动 `[scroll_x, scroll_y]`。
    pub scroll: [f32; 2],
    /// 键数。
    pub key_count: u16,
}

impl Case {
    fn new(label: &'static str, document: MidiDocument, tracks: usize) -> Self {
        Self {
            label,
            document,
            current_track: tracks,
            muted: vec![false; tracks],
            zoom: [1.0, ZOOM_Y],
            scroll: [0.0, 0.0],
            key_count: KEY_COUNT,
        }
    }
}

/// 用例 1：单轨单音（矩形位置 + 每侧 2px 边框 + 内部填充）。
pub fn single_note() -> Case {
    let mut document = MidiDocument::empty_with_tracks(2, 480);
    // x ∈ [8, 48)、y ∈ [20, 30)：填充 [10, 46) × [22, 28)，其余为边框。
    document.insert_note(1, NoteEvent::new(8, 48, 38, 100, 0));
    Case::new("单轨单音（矩形/边框）", document, 2)
}

/// 用例 2：同 `start_tick` 长/短音源顺序（Step 6a 主题）。
///
/// - 轨道 1：短音（40 tick，落在正常窗口流）先插入 → 源索引更小 → 必须由短音
///   占住像素：它在 x ∈ [38, 40) 的右边框必须可见（长音只提供填充）。
/// - 轨道 2：长音（600 tick > `LOOKBACK_TICKS`=512，落在长音符流）先插入 →
///   长音先画 → 短音的右边框被长音填充吃掉（x ∈ [38, 40) 为填充色）。
///
/// 两条轨同几何、同色系不同轨色，因此「边框 vs 填充」的差异只由绘制顺序决定，
/// 是对 GPU 源索引序 == CPU 文档源顺序的直接像素证据。
pub fn same_tick_order() -> Case {
    let mut document = MidiDocument::empty_with_tracks(3, 480);
    // 轨道 1：短先插（源索引 0），长后插（源索引 1）
    document.insert_note(1, NoteEvent::new(0, 40, 36, 100, 0));
    document.insert_note(1, NoteEvent::new(0, 600, 36, 100, 0));
    // 轨道 2：长先插（源索引 0），短后插（源索引 1）
    document.insert_note(2, NoteEvent::new(0, 600, 33, 100, 0));
    document.insert_note(2, NoteEvent::new(0, 40, 33, 100, 0));
    Case::new("同 tick 长/短音源顺序", document, 3)
}

/// 用例 3：多轨不同颜色 + 跨轨重叠 first-wins + 当前/静音排除。
///
/// - 轨道 0 与轨道 1 在 key 34（y ∈ [60, 70)）上重叠 x ∈ [40, 70)：
///   轨道索引小者在先 → 重叠区必须是轨道 0 的填充；
/// - 轨道 2 独占 key 31（y ∈ [90, 100)）→ 必须绘制；
/// - 轨道 3 是当前轨（key 37，y ∈ [30, 40)）→ 必须不绘制；
/// - 轨道 4 静音（key 28，y ∈ [120, 130)）→ 必须不绘制；
/// - 轨道 5 与当前轨同 key（37）但不同 x → 必须绘制（排除按轨道、不按区域）。
pub fn multi_track_overlap() -> Case {
    let mut document = MidiDocument::empty_with_tracks(6, 480);
    document.insert_note(0, NoteEvent::new(10, 70, 34, 100, 0));
    document.insert_note(1, NoteEvent::new(40, 120, 34, 100, 0));
    document.insert_note(2, NoteEvent::new(20, 60, 31, 100, 0));
    document.insert_note(3, NoteEvent::new(30, 80, 37, 100, 0));
    document.insert_note(4, NoteEvent::new(30, 80, 28, 100, 0));
    document.insert_note(5, NoteEvent::new(90, 130, 37, 100, 0));
    let mut case = Case::new("多轨颜色/重叠/当前静音排除", document, 6);
    case.current_track = 3;
    case.muted = vec![false, false, false, false, true, false];
    case
}

/// 用例 4：100% / 125% / 150% 物理相机（`zoom` 随 DPI 等比缩放），**刻度 0**。
///
/// tick 取奇数，使 125% 下 `x = tick * 1.25` 落在 `.75/.25`；150% 下落在 `.5`；
/// 125% 的 `zoom_y = 12.5` 让纵向 `y` 也落在 `.5`。这套几何**故意保留**了「填充
/// 边界恰好落在像素中心」的退化点（125% 下 `sx+sw-2 = 10.5` = 像素 10 的中心、
/// `sy+2 = 14.5` = 第 14 行中心；150% 下 `sx+2 = 6.5` = 像素 6 的中心），
/// 因为这些边界在真实 fractional zoom / 半像素滚动下必然出现，测试不能绕开。
///
/// 该退化点的归属现在由 `onion_note.wgsl::fs_main` 的**屏幕像素空间**判定保证：
/// `local = position.xy - rect_min`，`local == 2.0` / `local == size - 2.0` 一律
/// 归填充，与 CPU `fill_start` / `fill_end` 同义；判定里不含任何插值量，因此没有
/// 早期「同一行两个实例结论相反」的硬件插值噪声（历史证据见 gfx 的
/// `onion_note.wgsl` 头注释 BORDER_PIXEL_SPACE 块）。
///
/// `scroll` 保留 `[0.0, 0.0]`（真实零刻度）；半像素滚动另有
/// [`dpi_half_pixel_scroll`] 用例覆盖。
pub fn dpi(zoom_x: f32) -> Case {
    let mut document = MidiDocument::empty_with_tracks(4, 480);
    document.insert_note(1, NoteEvent::new(3, 10, 39, 100, 0));
    document.insert_note(1, NoteEvent::new(13, 18, 39, 100, 0));
    document.insert_note(2, NoteEvent::new(7, 16, 37, 100, 0));
    document.insert_note(2, NoteEvent::new(23, 26, 37, 100, 0));
    document.insert_note(3, NoteEvent::new(11, 22, 35, 100, 0));
    document.insert_note(3, NoteEvent::new(19, 32, 34, 100, 0));
    let mut case = Case::new("DPI 缩放", document, 4);
    case.zoom = [zoom_x, ZOOM_Y * zoom_x];
    case
}

/// 用例 5：125% + **半像素滚动**（`scroll = 0.3px`）。
///
/// 真实拖动滚动条后 scroll 会带任意小数；此例让四边都不落在像素中心
/// （最小距离 ≈ 0.05px），覆盖「非退化小数几何」这一半场景——
/// 与 [`dpi`] 的退化几何互补，两条都要求逐像素严格 0 差异。
pub fn dpi_half_pixel_scroll() -> Case {
    let mut case = dpi(1.25);
    case.label = "DPI 125% + 半像素滚动";
    case.scroll = [0.3, 0.3];
    case
}

/// 用例 6：**超宽音符**（100 万 tick = 100 万像素宽）的左侧 2px 边框。
///
/// 回归目的：任何「宽度相关」的边框收敛（例如曾经考虑过的 UV epsilon
/// `eps * sw`，1e-4 UV 在 1e6px 宽音符上等于 100px）都会把这条边框吃掉。
/// 现在的像素空间判定与宽度无关，边框恒为 2px。
pub fn wide_note() -> Case {
    let mut document = MidiDocument::empty_with_tracks(2, 480);
    document.insert_note(1, NoteEvent::new(0, 1_000_000, 38, 100, 0));
    Case::new("超宽音符（1e6 px）2px 边框", document, 2)
}
