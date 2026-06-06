//! 音轨总览视图 —— 显示所有音轨的音符在时间轴上的排列
//!
//! 使用 wgpu 实例化渲染（复用 NoteRenderer），不经过 iced Canvas。

pub mod canvas;
pub mod pattern_widget;

use iced_core::Point;
use lumino_core::Pattern;

use lumino_gfx::NoteInstance;

pub use canvas::ArrangementCanvas;
pub use pattern_widget::{PatternWidget, PatternWidgetState};

/// 音轨总览视口状态
#[derive(Debug, Clone)]
pub struct ArrangementViewport {
    /// 水平滚动（像素）
    pub scroll_x: f32,
    /// 垂直滚动（像素）
    pub scroll_y: f32,
    /// 水平缩放（像素/tick）
    pub zoom_x: f32,
    /// 每轨高度（像素）
    pub track_height: f32,
    /// Canvas 偏移（屏幕坐标）
    pub canvas_offset: Point,
    /// Canvas 尺寸
    pub canvas_size: Point,
    /// 总 tick 数
    pub total_ticks: u32,
}

impl Default for ArrangementViewport {
    fn default() -> Self {
        Self {
            scroll_x: 0.0,
            scroll_y: 0.0,
            zoom_x: 0.5,
            track_height: 32.0,
            canvas_offset: Point::new(0.0, 0.0),
            canvas_size: Point::new(800.0, 600.0),
            total_ticks: 0,
        }
    }
}

impl ArrangementViewport {
    /// 可见 tick 范围
    pub fn visible_tick_range(&self) -> (f32, f32) {
        let start = (self.scroll_x / self.zoom_x).max(0.0);
        let end = ((self.scroll_x + self.canvas_size.x) / self.zoom_x).max(start);
        (start, end)
    }

    /// 可见音轨索引范围
    pub fn visible_track_range(&self, track_count: usize) -> (usize, usize) {
        let start =
            ((self.scroll_y / self.track_height).floor().max(0.0) as usize).min(track_count);
        let end = (((self.scroll_y + self.canvas_size.y) / self.track_height).ceil() as usize)
            .min(track_count)
            .max(start);
        (start, end)
    }
}

/// 音轨总览视图
#[derive(Debug, Clone, Default)]
pub struct ArrangementView {
    /// 视口状态
    pub viewport: ArrangementViewport,
    /// Pattern 列表（音轨总览中的音符片段）
    pub patterns: Vec<Pattern>,
}

impl ArrangementView {
    pub fn new() -> Self {
        Self::default()
    }

    /// 生成音轨总览的音符实例
    ///
    /// 将每个音轨的音符映射为 NoteInstance：
    /// - x = tick（时间）
    /// - y = 音轨索引（替代音高）
    /// - 颜色根据音轨索引分配
    pub fn generate_instances(
        &self,
        track_notes: &std::collections::HashMap<usize, im::Vector<crate::editor::note::Note>>,
        track_order: &[usize],
        visible_tick_start: f32,
        visible_tick_end: f32,
        visible_track_start: usize,
        visible_track_end: usize,
    ) -> Vec<NoteInstance> {
        let mut instances = Vec::new();

        for (track_idx, track_id) in track_order.iter().enumerate() {
            // 只生成可见音轨的实例
            if track_idx < visible_track_start || track_idx >= visible_track_end {
                continue;
            }

            let Some(notes) = track_notes.get(track_id) else {
                continue;
            };

            let track_color = track_color(track_idx);

            for note in notes {
                // 视锥裁剪：只生成可见时间范围内的音符
                if note.tick + note.length < visible_tick_start || note.tick > visible_tick_end {
                    continue;
                }

                // 音轨总览中，y = 音轨索引
                // shader 中 y = (max_key_index - key) * zoom_y，
                // 所以 key=0 的音符会显示在最上方（y 最大）
                let y = track_idx as f32;

                instances.push(NoteInstance::new(note.tick, y, note.length, track_color));
            }
        }

        instances
    }
}

/// 为音轨索引分配颜色（使用预设调色板）
fn track_color(index: usize) -> [f32; 4] {
    const PALETTE: [[f32; 4]; 12] = [
        [0.90, 0.30, 0.30, 0.85], // 红
        [0.30, 0.70, 0.30, 0.85], // 绿
        [0.30, 0.50, 0.90, 0.85], // 蓝
        [0.90, 0.70, 0.20, 0.85], // 橙
        [0.70, 0.30, 0.80, 0.85], // 紫
        [0.20, 0.80, 0.80, 0.85], // 青
        [0.90, 0.50, 0.50, 0.85], // 粉红
        [0.50, 0.90, 0.30, 0.85], //  lime
        [0.30, 0.30, 0.70, 0.85], // 深蓝
        [0.90, 0.80, 0.30, 0.85], // 黄
        [0.60, 0.40, 0.20, 0.85], // 棕
        [0.50, 0.50, 0.50, 0.85], // 灰
    ];

    PALETTE[index % PALETTE.len()]
}
