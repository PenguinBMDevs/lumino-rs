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
pub(super) const MAIN_TRACK_NOTE_COLOR: [f32; 4] = [0.2, 0.55, 1.0, 1.0];

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
    border_width: u32,
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
            lumino_gfx::NoteInstance::new(0.0, 0u8, 0.0, [0.0; 4], border_width),
        );
        instances
            .par_iter_mut()
            .enumerate()
            .for_each(|(i, instance)| {
                let (tick, key, length) = visible_notes[i];
                *instance = lumino_gfx::NoteInstance::new(
                    tick,
                    key as u8,
                    length,
                    fixed_note_color,
                    border_width,
                );
            });
    } else {
        // 小数据量：顺序写入，避免并行分片开销
        instances.extend(visible_notes.iter().map(|(tick, key, length)| {
            lumino_gfx::NoteInstance::new(
                *tick,
                *key as u8,
                *length,
                fixed_note_color,
                border_width,
            )
        }));
    }

    // 预览音符的默认长度：优先使用上次放置的音符长度，其次使用精度设置的默认长度
    let preview_default_length = preview_ctx.last_note_length.unwrap_or(default_note_length);

    // 正在绘制的音符（Drawing 状态）— 预览音符用 new_preview（border_width 哨兵）
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
        instances.push(lumino_gfx::NoteInstance::new_preview(
            tick,
            *key as u8,
            length.max(snap_precision),
            MAIN_TRACK_NOTE_COLOR,
        ));
    } else if preview_ctx.hover_preview {
        // hover 预览音符（铅笔工具 + Idle 状态，跟随鼠标指针）
        instances.push(lumino_gfx::NoteInstance::new_preview(
            preview_ctx.cursor_tick,
            preview_ctx.cursor_key as u8,
            preview_default_length.max(snap_precision),
            MAIN_TRACK_NOTE_COLOR,
        ));
    }

    buffer.swap();
}

/// 主音轨音符描边宽度（固定 1 像素，用户要求）
const MAIN_TRACK_BORDER_WIDTH: u32 = 1;

/// 图片转 MIDI 主轨预览音符（tick, key, length）
pub(super) type I2mMainNote = (f32, u8, f32);
/// 图片转 MIDI 洋葱皮预览音符（tick, key, length, 调色板颜色）
pub(super) type I2mOnionNote = (f32, u8, f32, [f32; 4]);

/// 收集图片转 MIDI 预览音符（区域映射后）
///
/// 返回 `(主轨音符, 其他轨洋葱皮音符)`：
/// - 主轨 = `preview.tracks[0]`（颜色 0，插入时写入当前音轨）→ 实色
/// - 其他轨 = `preview.tracks[1..]` → 洋葱皮调色板颜色
pub(super) fn collect_i2m_preview_notes(
    editor: &crate::editor::Editor,
) -> (Vec<I2mMainNote>, Vec<I2mOnionNote>) {
    use lumino_editor_state::ImageToMidiMode;

    let i2m = &editor.editor_state.image_to_midi;
    if i2m.mode != ImageToMidiMode::Placing {
        return (Vec::new(), Vec::new());
    }
    let Some(preview) = &i2m.preview else {
        return (Vec::new(), Vec::new());
    };

    let mut main_notes = Vec::new();
    let mut onion_notes = Vec::new();
    for (track_idx, _) in preview.tracks.iter().enumerate() {
        let notes = i2m.track_screen_notes(track_idx);
        if track_idx == 0 {
            main_notes.extend(notes);
        } else {
            let color = lumino_extras::palette::current_track_color_f32(track_idx);
            onion_notes.extend(notes.into_iter().map(|(t, k, l)| (t, k, l, color)));
        }
    }
    (main_notes, onion_notes)
}

/// 构建图片转 MIDI 预览音符实例（追加到主音轨 buffer）
///
/// - 主轨音符（`main_notes`）：实色（`MAIN_TRACK_NOTE_COLOR`）
/// - 其他轨音符（`onion_notes`）：洋葱皮调色板颜色（调用方按音轨取色）
/// - `tick/key/length` 已由调用方完成区域 X 向等比映射
pub(super) fn build_i2m_preview_instances(
    buffer: &SwappableBuffer<lumino_gfx::NoteInstance>,
    main_notes: &[(f32, u8, f32)],
    onion_notes: &[(f32, u8, f32, [f32; 4])],
) {
    let total = main_notes.len() + onion_notes.len();
    if total == 0 {
        return;
    }
    let instances = unsafe { buffer.write_buffer() };
    instances.reserve(total);
    for (tick, key, length) in main_notes {
        instances.push(lumino_gfx::NoteInstance::new(
            *tick,
            *key,
            *length,
            MAIN_TRACK_NOTE_COLOR,
            MAIN_TRACK_BORDER_WIDTH,
        ));
    }
    for (tick, key, length, color) in onion_notes {
        instances.push(lumino_gfx::NoteInstance::new(
            *tick,
            *key,
            *length,
            *color,
            MAIN_TRACK_BORDER_WIDTH,
        ));
    }
    buffer.swap();
}
