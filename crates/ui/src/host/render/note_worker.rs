//! 洋葱皮辅助工具 & 主音符实例构建
//!
//! NoteWorker 已删除（方案 C：采集搬到渲染线程）。
//! 本文件仅保留：
//! - `ScrollVelocityTracker`：主线程滚动速度追踪（用于 overscan）
//! - `build_main_note_instances`：主音符实例构建

use std::time::{Duration, Instant};

use lumino_gfx::SwappableBuffer;

// ─── 滚动速度追踪器 ─────────────────────────────────────────────────────────

/// 滚动速度追踪器——测量 scroll_x 变化率来计算 overscan
#[derive(Debug)]
pub(crate) struct ScrollVelocityTracker {
    last_scroll_x: f32,
    last_time: Instant,
    samples: [f32; 5],
    sample_idx: usize,
}

impl ScrollVelocityTracker {
    pub fn new() -> Self {
        Self {
            last_scroll_x: 0.0,
            last_time: Instant::now(),
            samples: [0.0; 5],
            sample_idx: 0,
        }
    }

    /// 更新采样，返回当前峰值速度（ticks/sec）
    pub fn update(&mut self, scroll_x: f32, zoom_x: f32) -> f32 {
        let now = Instant::now();
        let dt = now.duration_since(self.last_time);
        self.last_time = now;

        let dx = scroll_x - self.last_scroll_x;
        self.last_scroll_x = scroll_x;

        if dt < Duration::from_millis(2) || zoom_x <= 0.0 {
            return self.peak_velocity();
        }

        let dt_sec = dt.as_secs_f32();
        let dx_ticks = dx / zoom_x;
        let velocity = if dx_ticks > 0.0 {
            dx_ticks / dt_sec
        } else {
            0.0
        };

        self.samples[self.sample_idx % 5] = velocity;
        self.sample_idx += 1;
        self.peak_velocity()
    }

    fn peak_velocity(&self) -> f32 {
        self.samples.iter().copied().fold(0.0, f32::max)
    }

    /// 计算需要的右侧 overscan ticks
    #[allow(dead_code)]
    pub fn overscan_ticks(&self, predict_ms: f32) -> f32 {
        self.peak_velocity() * predict_ms / 1000.0
    }
}

impl Default for ScrollVelocityTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ─── 主音轨主音符实例构建（主线程同步执行） ─────────────────────────────────

/// 主音轨已放置音符的固定蓝色（与洋葱皮取色区分）
const MAIN_TRACK_NOTE_COLOR: [f32; 4] = [0.2, 0.55, 1.0, 1.0];

/// 预览音符渲染上下文
///
/// 封装 hover 预览音符和 Drawing 预览音符所需的额外参数。
#[derive(Debug)]
pub(crate) struct PreviewNoteContext {
    /// 是否渲染 hover 预览音符（铅笔工具 + Idle 状态 + 光标在卷帘区域内 + 菜单未打开）
    pub hover_preview: bool,
    /// 光标对应的吸附后 tick
    pub cursor_tick: f32,
    /// 光标对应的 key
    pub cursor_key: u16,
    /// 上次放置的音符长度（如果有），用于预览矩形和 Drawing 默认长度
    pub last_note_length: Option<f32>,
}

pub(super) fn build_main_note_instances(
    buffer: &SwappableBuffer<lumino_gfx::NoteInstance>,
    visible_notes: &[(f32, u16, f32)],
    edit_state: &crate::editor::editor_state::EditState,
    default_note_length: f32,
    snap_precision: f32,
    preview_ctx: &PreviewNoteContext,
) {
    use rayon::prelude::*;

    // 主音轨已放置音符统一使用蓝色，与洋葱皮（取自调色板）明确区分
    let fixed_note_color: [f32; 4] = MAIN_TRACK_NOTE_COLOR;
    const PARALLEL_THRESHOLD: usize = 500;

    let instances = unsafe { buffer.write_buffer() };
    instances.clear();
    instances.reserve(visible_notes.len() + 2);

    // 大数据量：并行直接写入 SwappableBuffer，避免中间 Vec 分配
    if visible_notes.len() >= PARALLEL_THRESHOLD {
        instances.resize(
            visible_notes.len(),
            lumino_gfx::NoteInstance::new(0.0, 0.0, 0.0, [0.0; 4]),
        );
        instances
            .par_iter_mut()
            .enumerate()
            .for_each(|(i, instance)| {
                let (tick, key, length) = visible_notes[i];
                *instance =
                    lumino_gfx::NoteInstance::new(tick, key as f32, length, fixed_note_color);
            });
    } else {
        // 小数据量：顺序写入，避免并行分片开销
        instances.extend(visible_notes.iter().map(|(tick, key, length)| {
            lumino_gfx::NoteInstance::new(*tick, *key as f32, *length, fixed_note_color)
        }));
    }

    // 预览音符的默认长度：优先使用上次放置的音符长度，其次使用精度设置的默认长度
    let preview_default_length = preview_ctx.last_note_length.unwrap_or(default_note_length);

    // 正在绘制的音符（Drawing 状态）
    if let crate::editor::editor_state::EditState::Drawing {
        start_tick,
        key,
        current_tick,
    } = edit_state
    {
        let (tick, length) = if *current_tick > *start_tick {
            (*start_tick, *current_tick - *start_tick)
        } else if *current_tick < *start_tick {
            (*current_tick, *start_tick - *current_tick)
        } else {
            // 未拖动时使用 last_note_length 或 default_note_length
            (*start_tick, preview_default_length)
        };
        instances.push(lumino_gfx::NoteInstance::new_with_flags(
            tick,
            *key as f32,
            length.max(snap_precision),
            MAIN_TRACK_NOTE_COLOR,
            lumino_gfx::FLAG_PREVIEW,
        ));
    } else if preview_ctx.hover_preview {
        // hover 预览音符（铅笔工具 + Idle 状态，跟随鼠标指针）
        instances.push(lumino_gfx::NoteInstance::new_with_flags(
            preview_ctx.cursor_tick,
            preview_ctx.cursor_key as f32,
            preview_default_length.max(snap_precision),
            MAIN_TRACK_NOTE_COLOR,
            lumino_gfx::FLAG_PREVIEW,
        ));
    }

    buffer.swap();
}
