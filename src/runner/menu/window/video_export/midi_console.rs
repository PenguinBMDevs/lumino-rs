//! MidiConsole 风格视频渲染（CPU：字符网格 → ab_glyph 栅格化 PNG）
//!
//! 复刻 MidiConsole（by Zacksony）的终端像素网格风格：
//! - 顶部统计行：播放速度 / 曲速 / 拍号 / TPQ / TICK / NOTES / EVENTS
//! - 控制面板表头 + 逐通道（CH01..CH16）行：半块键盘条 + 控制面板
//!   （PC/VOL/EXP/PAN/P.BEND/P.RANGE/MOD/HOLD/CUT/RESO/ATT/DEC/REL）
//! - ALL 合并行：所有通道按键的 OR
//!
//! 风格本质为「等宽终端字符网格 + ANSI 真彩」，因此本渲染器分两步：
//! 1. [`MidiConsoleRenderer::render`] 把当前 tick 的状态写进一个
//!    `ROWS×COLS` 的 [`Cell`] 字符网格（每格 = 字符 + 前景色 + 背景色）；
//! 2. [`render_midicomsole_frame`] 用 `ab_glyph`（本仓已有依赖）把字符网格
//!    按正确 cell 比例栅格化成 BGRA 像素（含半块字符）。
//!
//! 键盘条采用原版技巧：每格用 `▌`（左半块）字形画「左键」颜色，
//! 单元格背景填「右键」颜色，从而在一个字符内同时呈现两个键。

use lumino_message::events::window::video::{MidiConsoleBackend, MidiConsoleConfig};

use super::counter_stats;

/// 逻辑网格（与原版 148×40 终端一致）
const COLS: u32 = 148;
const ROWS: u32 = 40;
/// 键盘条：128 键 → 64 个半块单元（左键/右键各占半宽）
const KEYBOARD_COL: u32 = 5;
const KEYBOARD_CELLS: u32 = 64;
/// 控制面板字段数（PC/VOL/EXP/PAN/P.BEND/P.RANGE/MOD/HOLD/CUT/RESO/ATT/DEC/REL）
const CONTROL_FIELDS: usize = 13;

/// 控制字段列起始（逻辑列），位于键盘条右侧、与键盘同行横向对齐
const CTRL_COLS: [u32; CONTROL_FIELDS] =
    [71, 76, 81, 86, 91, 99, 107, 112, 117, 122, 127, 132, 137];

/// 调色板（近似 ANSI 真彩终端）
const BG: [u8; 3] = [12, 12, 14];
const TEXT: [u8; 3] = [200, 200, 210];
const LABEL: [u8; 3] = [190, 196, 216];
const WARN: [u8; 3] = [210, 55, 55];
/// 黑键 / 白键未按下时的底色
const KEY_BLACK: [u8; 3] = [72, 76, 92];
const KEY_WHITE: [u8; 3] = [104, 110, 126];

/// 字符网格中的一个单元
#[derive(Clone, Copy)]
pub struct Cell {
    ch: char,
    fg: [u8; 3],
    bg: [u8; 3],
}

impl Cell {
    fn blank() -> Self {
        Cell {
            ch: ' ',
            fg: TEXT,
            bg: BG,
        }
    }
}

/// MidiConsole 风格渲染配置（runner 内部使用，由事件层 `MidiConsoleConfig` 转换）
#[derive(Debug, Clone)]
pub struct MidiConsoleRenderConfig {
    pub render_backend: MidiConsoleBackend,
    pub show_control_panel: bool,
    pub keyboard_fade_frames: u32,
    pub control_fade_frames: u32,
    pub warm_key_color: [u8; 3],
}

impl From<&MidiConsoleConfig> for MidiConsoleRenderConfig {
    fn from(c: &MidiConsoleConfig) -> Self {
        Self {
            render_backend: c.render_backend,
            show_control_panel: c.show_control_panel,
            keyboard_fade_frames: c.keyboard_fade_frames.max(1),
            control_fade_frames: c.control_fade_frames.max(1),
            warm_key_color: c.warm_key_color,
        }
    }
}

/// MidiConsole 渲染器（状态跨帧保持：淡出计时器 / 游标 / 控制状态）
#[derive(Clone)]
pub struct MidiConsoleRenderer {
    /// 逐通道音符（按 start 排序）：`(start_tick, end_tick, key)`
    channel_notes: [Vec<(u32, u32, u8)>; 16],
    /// 每通道已扫描游标（第一个 start > tick 的索引）
    note_cursor: [usize; 16],
    /// 当前逐通道按下的键（用于键盘条）
    pressed: [[bool; 128]; 16],
    /// 活跃音符 `(end_tick, channel, key)`，用于增量移除
    active: Vec<(u32, u8, u8)>,
    /// 逐通道 ProgramChange（0-127）
    ch_program: [u8; 16],
    /// 逐通道 CC 值 `[channel][controller]`
    ch_cc: [[u8; 128]; 16],
    /// 逐通道 PitchBend（有符号，中心 0）
    ch_pitch: [i32; 16],
    /// control_events 扫描游标
    cc_cursor: usize,
    /// 键盘按键亮度水平（0=熄灭底色，1=完全点亮暖色）：`[行][key]`，行 0 = ALL，1..16 = CH01..CH16
    /// 每帧向目标（按下=1 / 松开=0）连续趋近，实现亮灭平滑过渡动画
    key_level: [[f32; 128]; 17],
    /// 控制面板变化高亮亮度水平（0=常态，1=最强高亮）：`[channel][field]`
    /// 变化瞬间置 1，随后每帧趋向 0，实现高亮淡出
    ctrl_level: [[f32; CONTROL_FIELDS]; 16],
    /// 累计已开始音符总数（NOTES 统计）
    note_count: u64,
    /// 上一帧 tick（回退检测）
    last_tick: u32,
    /// 渲染配置
    config: MidiConsoleRenderConfig,
}

// ───────────────────────── 字符网格 → 像素 ─────────────────────────

mod cpu;
mod font;
mod gpu;
mod renderer;

#[cfg(test)]
mod tests;

pub use cpu::{MidiConsoleFrameArgs, render_midicomsole_frame};
pub use gpu::render_midicomsole_frame_gpu;
