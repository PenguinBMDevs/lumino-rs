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
//!    按正确 cell 比例栅格化成 BGRA 像素（含半块字符 ▌▀▄▐░▒▓）。
//!
//! 键盘条采用原版技巧：每格用 `▌`（左半块）字形画「左键」颜色，
//! 单元格背景填「右键」颜色，从而在一个字符内同时呈现两个键。

use std::path::PathBuf;

use ab_glyph::{Font, FontArc, FontVec, Point, PxScale, ScaleFont};
use lumino_gfx::midiconsole_renderer::{CellGpu, MidiconsoleGpuContext, pack_rgb};
use lumino_message::events::window::video::MidiConsoleConfig;
use lumino_midi_loader::MidiDocument;

/// 逻辑网格（与原版 148×40 终端一致）
const COLS: u32 = 148;
const ROWS: u32 = 40;
/// 键盘条：128 键 → 64 个半块单元（左键/右键各占半宽）
const KEYBOARD_COL: u32 = 5;
const KEYBOARD_CELLS: u32 = 64;
/// 控制面板字段数（PC/VOL/EXP/PAN/P.BEND/P.RANGE/MOD/HOLD/CUT/RESO/ATT/DEC/REL）
const CONTROL_FIELDS: usize = 13;

/// 控制字段列起始（逻辑列），与表头标签对齐
const CTRL_COLS: [u32; CONTROL_FIELDS] = [
    71, 76, 81, 86, 91, 99, 107, 112, 117, 122, 127, 132, 137,
];

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
    pub show_control_panel: bool,
    pub keyboard_fade_frames: u32,
    pub control_fade_frames: u32,
    pub warm_key_color: [u8; 3],
}

impl From<&MidiConsoleConfig> for MidiConsoleRenderConfig {
    fn from(c: &MidiConsoleConfig) -> Self {
        Self {
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

impl MidiConsoleRenderer {
    /// 从文档预构建逐通道音符索引
    pub fn new(document: &MidiDocument, config: &MidiConsoleRenderConfig) -> Self {
        let mut channel_notes: [Vec<(u32, u32, u8)>; 16] = core::array::from_fn(|_| Vec::new());
        for track in &document.notes {
            for n in track.iter() {
                let ch = (n.channel & 0x0F) as usize;
                if ch < 16 {
                    channel_notes[ch].push((n.start_tick, n.end_tick, n.key));
                }
            }
        }
        for v in &mut channel_notes {
            v.sort_by_key(|x| x.0);
        }
        Self {
            channel_notes,
            note_cursor: [0; 16],
            pressed: [[false; 128]; 16],
            active: Vec::new(),
            ch_program: [0; 16],
            ch_cc: [[0; 128]; 16],
            ch_pitch: [0; 16],
            cc_cursor: 0,
            key_level: [[0.0; 128]; 17],
            ctrl_level: [[0.0; CONTROL_FIELDS]; 16],
            note_count: 0,
            last_tick: 0,
            config: config.clone(),
        }
    }

    /// 将渲染状态重置为干净态（tick 回退/跳变时）
    fn reset_state(&mut self) {
        self.note_cursor = [0; 16];
        self.pressed = [[false; 128]; 16];
        self.active.clear();
        self.ch_program = [0; 16];
        self.ch_cc = [[0; 128]; 16];
        self.ch_pitch = [0; 16];
        self.cc_cursor = 0;
        self.key_level = [[0.0; 128]; 17];
        self.ctrl_level = [[0.0; CONTROL_FIELDS]; 16];
        self.note_count = 0;
        self.last_tick = 0;
    }

    /// 增量推进到指定 tick：维护逐通道按下键集合与控制状态
    fn advance(&mut self, document: &MidiDocument, tick: u32) {
        if tick < self.last_tick {
            self.reset_state();
        }

        // 1. 逐通道音符：二分推进游标，新增开始音符 / 移除已结束音符
        for ch in 0..16 {
            let notes = &self.channel_notes[ch];
            while self.note_cursor[ch] < notes.len() && notes[self.note_cursor[ch]].0 <= tick {
                let (s, e, k) = notes[self.note_cursor[ch]];
                self.note_cursor[ch] += 1;
                self.active.push((e, ch as u8, k));
                self.pressed[ch][k as usize] = true;
                self.note_count += 1;
                let _ = s;
            }
        }
        // 移除已结束音符，清除对应 pressed
        self.active.retain(|(e, ch, k)| {
            if *e <= tick {
                self.pressed[*ch as usize][*k as usize] = false;
                false
            } else {
                true
            }
        });
        // 2. 控制事件（CC / PC / PB），驱动控制面板高亮
        let ces = &document.control_events;
        while self.cc_cursor < ces.len() && ces[self.cc_cursor].tick <= tick {
            let ev = &ces[self.cc_cursor];
            let ch = ev.channel as usize;
            if ch < 16 {
                match ev.kind {
                    0 => {
                        let (c, v) = ev.as_control_change();
                        self.ch_cc[ch][c as usize] = v;
                        if let Some(f) = cc_field_index(c) {
                            self.ctrl_level[ch][f] = 1.0;
                        }
                    }
                    1 => {
                        let p = ev.as_program_change();
                        self.ch_program[ch] = p;
                        self.ctrl_level[ch][0] = 1.0;
                    }
                    2 => {
                        let pb = ev.as_pitch_bend();
                        self.ch_pitch[ch] = (pb as i32) - 8192;
                        self.ctrl_level[ch][4] = 1.0;
                    }
                    _ => {}
                }
            }
            self.cc_cursor += 1;
        }

        // 3. 亮度水平连续趋近：按键 → 趋向 1（亮起渐变），松开 → 趋向 0（熄灭渐变）；
        //    控制面板变化 → 瞬间置 1，随后趋向 0（高亮淡出）。实现平滑过渡动画。
        let krate = 1.0 / self.config.keyboard_fade_frames.max(1) as f32;
        let crate_rate = 1.0 / self.config.control_fade_frames.max(1) as f32;
        for ch in 0..16 {
            for k in 0..128usize {
                let tgt = if self.pressed[ch][k] { 1.0 } else { 0.0 };
                self.key_level[ch + 1][k] = ramp(self.key_level[ch + 1][k], tgt, krate);
            }
            for f in 0..CONTROL_FIELDS {
                self.ctrl_level[ch][f] = ramp(self.ctrl_level[ch][f], 0.0, crate_rate);
            }
        }
        // ALL 行亮度 = 各通道最大值
        for k in 0..128usize {
            let mut m = 0.0f32;
            for ch in 0..16 {
                m = m.max(self.key_level[ch + 1][k]);
            }
            self.key_level[0][k] = m;
        }

        self.last_tick = tick;
    }

    /// 当前通道控制面板数值（与 CONTROL_FIELDS 顺序一致）
    fn control_values(&self, ch: usize) -> [i32; CONTROL_FIELDS] {
        let mut v = [0i32; CONTROL_FIELDS];
        v[0] = self.ch_program[ch] as i32; // PC
        v[1] = self.ch_cc[ch][7] as i32; // VOL (CC7)
        v[2] = self.ch_cc[ch][11] as i32; // EXP (CC11)
        v[3] = self.ch_cc[ch][10] as i32; // PAN (CC10)
        v[4] = self.ch_pitch[ch]; // P.BEND
        v[5] = self.ch_cc[ch][6] as i32; // P.RANGE (CC6)
        v[6] = self.ch_cc[ch][1] as i32; // MOD (CC1)
        v[7] = self.ch_cc[ch][64] as i32; // HOLD (CC64)
        v[8] = self.ch_cc[ch][74] as i32; // CUT (CC74)
        v[9] = self.ch_cc[ch][71] as i32; // RESO (CC71)
        v[10] = self.ch_cc[ch][73] as i32; // ATT (CC73)
        v[11] = self.ch_cc[ch][75] as i32; // DEC (CC75)
        v[12] = self.ch_cc[ch][72] as i32; // REL (CC72)
        v
    }

    /// 计算某键在指定行（ch_index: 0..15 通道, 16 = ALL）的显示颜色
    fn key_color_for(&self, ch_index: usize, k: usize) -> [u8; 3] {
        if k >= 128 {
            return [18, 18, 20];
        }
        let level = if ch_index == 16 {
            self.key_level[0][k]
        } else {
            self.key_level[ch_index + 1][k]
        };
        key_color(k, level, self.config.warm_key_color)
    }

    /// 把当前 tick 的状态写入字符网格
    pub fn render(&mut self, grid: &mut [Cell], document: &MidiDocument, tick: u32, ppq: u32, fps: u32) {
        self.advance(document, tick);
        let gw = COLS as usize;
        for c in grid.iter_mut() {
            *c = Cell::blank();
        }

        self.draw_header(grid, document, tick, ppq, fps);
        self.draw_stats(grid, document, tick, ppq, fps);
        self.draw_control_header(grid);

        for ch in 0..16usize {
            let kb_row = 3 + ch * 2;
            let ctrl_row = kb_row + 1;
            self.draw_keyboard_row(grid, kb_row as u32, ch as usize);
            if self.config.show_control_panel {
                self.draw_control_row(grid, ctrl_row as u32, ch as usize);
            }
        }
        // ALL 合并行
        self.draw_keyboard_row(grid, (3 + 16 * 2) as u32, 16);
        let _ = gw;
    }

    fn draw_header(&self, grid: &mut [Cell], _document: &MidiDocument, _tick: u32, _ppq: u32, _fps: u32) {
        set_text(grid, 0, 1, "LUMINO MIDICONSOLE", [220, 220, 230], BG);
        // 显式标注：CH01..CH16 行即 MIDI 通道 1..16（非轨道），按键亮起按通道独立检测。
        // 仅用 ASCII（Consolas/DejaVu 保证有字形），避免 ▶/·/中文 等缺失字形导致空白。
        set_text(grid, 0, 20, "- CH1-16 = MIDI CHANNEL", [150, 170, 210], BG);
        set_text(grid, 0, COLS - 10, "> PLAYING", [120, 220, 120], BG);
    }

    fn draw_stats(&self, grid: &mut [Cell], document: &MidiDocument, tick: u32, ppq: u32, fps: u32) {
        let bpm = super::counter_stats::current_bpm(&document.tempo_changes, tick);
        let (num, den) = super::counter_stats::current_time_signature(&document.time_signatures, tick);
        let speed = play_speed(ppq, bpm, fps);
        let notes = self.note_count;
        let events = document.control_events.len() as u64;
        let s = format!(
            "SPD {:.2}x  BPM {:.1}  {}/{}  TPQ {}  TICK {}  NOTES {}  EVENTS {}",
            speed, bpm, num, den, ppq, tick, notes, events
        );
        set_text(grid, 1, 1, &s, [180, 180, 190], BG);
    }

    fn draw_control_header(&self, grid: &mut [Cell]) {
        let labels = [
            "PC", "VOL", "EXP", "PAN", "P.BEND", "P.RANGE", "MOD", "HOLD", "CUT", "RESO", "ATT", "DEC",
            "REL",
        ];
        for f in 0..CONTROL_FIELDS {
            set_text(grid, 2, CTRL_COLS[f], labels[f], LABEL, BG);
        }
    }

    /// 绘制一行键盘条（ch_index: 0..15 通道，16 = ALL）
    fn draw_keyboard_row(&self, grid: &mut [Cell], row: u32, ch_index: usize) {
        let label = if ch_index == 16 {
            "ALL".to_string()
        } else {
            format!("CH{:02}", ch_index + 1)
        };
        set_text(grid, row, 0, &label, [210, 210, 220], BG);

        for i in 0..KEYBOARD_CELLS {
            let k0 = (i * 2) as usize;
            let k1 = (i * 2 + 1) as usize;
            let c = KEYBOARD_COL + i;
            let col0 = self.key_color_for(ch_index, k0);
            let col1 = self.key_color_for(ch_index, k1);
            // 左半块字形 `▌` 画左键颜色，背景填右键颜色
            set_cell(grid, row, c, '▌', col0, col1);
        }
    }

    /// 绘制一行控制面板数值
    fn draw_control_row(&self, grid: &mut [Cell], row: u32, ch: usize) {
        let vals = self.control_values(ch);
        for f in 0..CONTROL_FIELDS {
            let level = self.ctrl_level[ch][f];
            let (txt, col) = format_field(f, vals[f], level);
            set_text(grid, row, CTRL_COLS[f], &txt, col, BG);
        }
    }
}

/// 播放速度（倍速）：每帧推进 1 个 tick，1x 速度下每秒推进 (ppq * bpm / 60) 个 tick
fn play_speed(ppq: u32, bpm: f64, fps: u32) -> f64 {
    if bpm <= 0.0 {
        return 0.0;
    }
    (fps as f64) / (ppq as f64 * bpm / 60.0)
}

/// 逐通道控制面板字段格式化
fn format_field(f: usize, v: i32, level: f32) -> (String, [u8; 3]) {
    // 高亮强度随 ctrl_level 从常态色平滑过渡到告警红（控制变化高亮淡出动画）
    let col = mix([205, 205, 185], WARN, level.clamp(0.0, 1.0));
    let s = match f {
        0 => {
            if v < 0 {
                "---".to_string()
            } else {
                format!("{:>3}", v + 1)
            }
        } // PC（program + 1）
        1 => format!("{:>3}", v), // VOL
        2 => format!("{:>3}", v), // EXP
        3 => format!("{:>3}", v), // PAN
        4 => {
            if v == i32::MIN {
                "----".to_string()
            } else {
                format!("{:>4}", v)
            }
        } // P.BEND
        5 => format!("{:>3}", v), // P.RANGE
        6 => format!("{:>3}", v), // MOD
        7 => format!("{:>3}", v), // HOLD
        8 => format!("{:>3}", v), // CUT
        9 => format!("{:>3}", v), // RESO
        10 => format!("{:>3}", v), // ATT
        11 => format!("{:>3}", v), // DEC
        12 => format!("{:>3}", v), // REL
        _ => "---".to_string(),
    };
    (s, col)
}

/// CC 控制器号 → 控制面板字段索引（用于高亮触发）
fn cc_field_index(c: u8) -> Option<usize> {
    Some(match c {
        7 => 1,
        11 => 2,
        10 => 3,
        6 => 5,
        1 => 6,
        64 => 7,
        74 => 8,
        71 => 9,
        73 => 10,
        75 => 11,
        72 => 12,
        _ => return None,
    })
}

/// 逐通道按键（key%12 映射）是否为黑键
fn is_black_key(k: usize) -> bool {
    matches!(k % 12, 1 | 3 | 6 | 8 | 10)
}

/// 键颜色：底色（黑/白键区分）按亮度水平 level（0=熄灭，1=点亮）平滑过渡到暖色，
/// 因此按键的亮起与熄灭都是连续渐变动画。
fn key_color(k: usize, level: f32, warm: [u8; 3]) -> [u8; 3] {
    let base: [u8; 3] = if is_black_key(k) { KEY_BLACK } else { KEY_WHITE };
    let t = level.clamp(0.0, 1.0);
    [
        lerp(base[0], warm[0], t),
        lerp(base[1], warm[1], t),
        lerp(base[2], warm[2], t),
    ]
}

fn lerp(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).clamp(0.0, 255.0) as u8
}

/// 两个 RGB 颜色按 t 线性混合
fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    [lerp(a[0], b[0], t), lerp(a[1], b[1], t), lerp(a[2], b[2], t)]
}

/// 向目标值以固定步长连续趋近（用于亮度过渡动画）
#[inline]
fn ramp(cur: f32, tgt: f32, rate: f32) -> f32 {
    if cur < tgt {
        (cur + rate).min(tgt)
    } else if cur > tgt {
        (cur - rate).max(tgt)
    } else {
        cur
    }
}

// ───────────────────────── 字符网格 → 像素 ─────────────────────────

#[inline]
fn cell_idx(r: u32, c: u32) -> usize {
    (r * COLS + c) as usize
}

fn set_cell(grid: &mut [Cell], r: u32, c: u32, ch: char, fg: [u8; 3], bg: [u8; 3]) {
    if r < ROWS && c < COLS {
        grid[cell_idx(r, c)] = Cell { ch, fg, bg };
    }
}

fn set_text(grid: &mut [Cell], r: u32, c: u32, text: &str, fg: [u8; 3], bg: [u8; 3]) {
    for (i, ch) in text.chars().enumerate() {
        set_cell(grid, r, c + i as u32, ch, fg, bg);
    }
}

/// 用 `ab_glyph` 加载一个等宽字体（优先 Consolas / DejaVuSansMono）
fn load_monospace_font() -> Option<FontArc> {
    let candidates: Vec<PathBuf> = if cfg!(windows) {
        let dir = std::env::var("SystemRoot")
            .unwrap_or_else(|_| "C:\\Windows".to_string())
            + "\\Fonts\\";
        vec![
            PathBuf::from(dir.clone() + "consola.ttf"),
            PathBuf::from(dir.clone() + "consolab.ttf"),
            PathBuf::from(dir.clone() + "couri.ttf"),
            PathBuf::from(dir + "arial.ttf"),
        ]
    } else {
        vec![
            PathBuf::from("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"),
            PathBuf::from("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"),
            PathBuf::from("/System/Library/Fonts/Supplemental/Courier New.ttf"),
            PathBuf::from("/Library/Fonts/Courier New.ttf"),
        ]
    };
    for p in &candidates {
        if let Ok(bytes) = std::fs::read(p) {
            if let Ok(f) = FontVec::try_from_vec(bytes) {
                return Some(FontArc::from(f));
            }
        }
    }
    None
}

/// 把字符网格栅格化为 BGRA 帧（按 cell 比例，含半块字符字形）
pub fn render_midicomsole_frame(
    renderer: &mut MidiConsoleRenderer,
    frame: &mut [u8],
    frame_width: u32,
    frame_height: u32,
    document: &MidiDocument,
    tick: u32,
    ppq: u32,
    fps: u32,
) {
    let gw = COLS as usize;
    let gh = ROWS as usize;
    let fw = frame_width as usize;
    let fh = frame_height as usize;
    let out_len = fw * fh * 4;
    if frame.len() < out_len {
        return;
    }

    // 1) 渲染字符网格
    let mut grid = vec![Cell::blank(); gw * gh];
    renderer.render(&mut grid, document, tick, ppq, fps);

    let cell_w = frame_width as f32 / COLS as f32;
    let cell_h = frame_height as f32 / ROWS as f32;

    // 2) 栅格化（参照本仓 ui-editor 的 ab_glyph 用法：glyph_id + outline_glyph）
    if let Some(font) = load_monospace_font() {
        let scale = PxScale { x: cell_h, y: cell_h };
        let scaled = font.as_scaled(scale);
        let ascent = scaled.ascent();
        let descent = scaled.descent();
        let glyph_h = ascent - descent;
        for r in 0..gh {
            for c in 0..gw {
                let cell = grid[r * gw + c];
                // 先填充背景
                fill_rect(frame, fw, fh, c as f32 * cell_w, r as f32 * cell_h, cell_w, cell_h, cell.bg);
                if cell.ch != ' ' {
                    let gid = font.glyph_id(cell.ch);
                    let ha = scaled.h_advance(gid);
                    let off_x = (cell_w - ha) / 2.0;
                    let off_y = (cell_h - glyph_h) / 2.0;
                    let baseline_x = c as f32 * cell_w + off_x;
                    let baseline_y = r as f32 * cell_h + off_y + ascent;
                    let glyph = gid.with_scale_and_position(scale, Point { x: baseline_x, y: baseline_y });
                    if let Some(outline) = font.outline_glyph(glyph) {
                        // 关键：ab_glyph 的 draw 回调坐标是「相对字形包围盒左上角」，
                        // 必须叠加 px_bounds().min 才是帧内绝对像素（与本仓 text_tool 完全一致）
                        let b = outline.px_bounds();
                        outline.draw(|px, py, cov| {
                            let x = (px as f32 + b.min.x).round() as i32;
                            let y = (py as f32 + b.min.y).round() as i32;
                            if x < 0 || y < 0 {
                                return;
                            }
                            let di = (y as usize * fw + x as usize) * 4;
                            if di + 3 >= frame.len() {
                                return;
                            }
                            let a = cov.clamp(0.0, 1.0) as f32;
                            frame[di] = blend(cell.bg[0], cell.fg[0], a);
                            frame[di + 1] = blend(cell.bg[1], cell.fg[1], a);
                            frame[di + 2] = blend(cell.bg[2], cell.fg[2], a);
                            frame[di + 3] = 255;
                        });
                    }
                }
            }
        }
    } else {
        // 降级：无字体时把非空格当作前景色块填充（保证仍有可见输出）
        for r in 0..gh {
            for c in 0..gw {
                let cell = grid[r * gw + c];
                let col = if cell.ch == ' ' { cell.bg } else { cell.fg };
                fill_rect(frame, fw, fh, c as f32 * cell_w, r as f32 * cell_h, cell_w, cell_h, col);
            }
        }
    }

    // 3) 复古终端 CRT 后处理：静态扫描线 + 随 tick 移动的高亮扫描带（动态发光）
    apply_crt_effect(frame, fw, fh, tick);
}

/// 与 [`render_midicomsole_frame`] 同签名的 GPU 加速版本。
///
/// 复用 CPU 廉价网格构建（状态机逻辑，不慢），仅把昂贵的 ab_glyph 逐帧描边与
/// CRT 逐像素后处理搬上 GPU（`lumino_gfx::midiconsole_renderer`）。
/// 任意 GPU 初始化/渲染失败都安全回退到 CPU 路径，保证导出不中断。
pub fn render_midicomsole_frame_gpu(
    renderer: &mut MidiConsoleRenderer,
    frame: &mut [u8],
    frame_width: u32,
    frame_height: u32,
    document: &MidiDocument,
    tick: u32,
    ppq: u32,
    fps: u32,
) {
    let gw = COLS as usize;
    let gh = ROWS as usize;
    let fw = frame_width as usize;
    let fh = frame_height as usize;
    let out_len = fw * fh * 4;
    if frame.len() < out_len {
        return;
    }

    // 1) 复用 CPU 网格构建（廉价）
    let mut grid = vec![Cell::blank(); gw * gh];
    renderer.render(&mut grid, document, tick, ppq, fps);

    // 2) 转换为 GPU 单元（0xRRGGBB）
    let mut cells: Vec<CellGpu> = Vec::with_capacity(gw * gh);
    for c in &grid {
        cells.push(CellGpu {
            ch: c.ch as u32,
            fg: pack_rgb(c.fg[0], c.fg[1], c.fg[2]),
            bg: pack_rgb(c.bg[0], c.bg[1], c.bg[2]),
            _pad: 0,
        });
    }

    // 3) GPU 渲染（线程内缓存设备/渲染器，失败回退 CPU）
    let rgba = GPU_CTX.with(|g| {
        if GPU_DISABLED.with(|d| *d.borrow()) {
            return None;
        }
        let mut guard = g.borrow_mut();
        let need_rebuild = guard
            .as_ref()
            .map_or(true, |c| c.width != frame_width || c.height != frame_height);
        if need_rebuild {
            match build_gpu_ctx(frame_width, frame_height) {
                Some(ctx) => *guard = Some(ctx),
                None => {
                    GPU_DISABLED.with(|d| *d.borrow_mut() = true);
                    return None;
                }
            }
        }
        let ctx = guard.as_mut().expect("GPU 上下文已建立");
        Some(ctx.ctx.render_frame(&cells, tick))
    });

    match rgba {
        Some(rgba) => {
            // RGBA → BGRA（导出编码器按 "bgra" 消费 MidiConsole 帧）
            for y in 0..fh {
                for x in 0..fw {
                    let si = (y * fw + x) * 4;
                    let di = si;
                    frame[di] = rgba[si + 2];
                    frame[di + 1] = rgba[si + 1];
                    frame[di + 2] = rgba[si];
                    frame[di + 3] = 255;
                }
            }
        }
        None => render_midicomsole_frame(
            renderer, frame, frame_width, frame_height, document, tick, ppq, fps,
        ),
    }
}

/// 线程内缓存的 GPU 上下文（来自 `lumino_gfx`，按分辨率缓存）
struct GpuCtx {
    width: u32,
    height: u32,
    ctx: MidiconsoleGpuContext,
}

thread_local! {
    /// 当前线程的 GPU 上下文（导出在独立后台线程运行，单例缓存即可）
    static GPU_CTX: std::cell::RefCell<Option<GpuCtx>> = std::cell::RefCell::new(None);
    /// GPU 不可用时置位，避免每帧重复尝试创建适配器
    static GPU_DISABLED: std::cell::RefCell<bool> = std::cell::RefCell::new(false);
}

/// 创建 GPU 上下文（无可用适配器时返回 `None`）
fn build_gpu_ctx(width: u32, height: u32) -> Option<GpuCtx> {
    let ctx = MidiconsoleGpuContext::new(width, height)?;
    Some(GpuCtx {
        width,
        height,
        ctx,
    })
}

/// 背景色与前景色按覆盖率混合（BGRA 顺序）
#[inline]
fn blend(bg: u8, fg: u8, a: f32) -> u8 {
    (fg as f32 * a + bg as f32 * (1.0 - a)).clamp(0.0, 255.0) as u8
}

/// 填充矩形（BGRA）
fn fill_rect(
    frame: &mut [u8],
    fw: usize,
    fh: usize,
    x0: f32,
    y0: f32,
    w: f32,
    h: f32,
    color: [u8; 3],
) {
    let x0 = x0.max(0.0) as i64;
    let y0 = y0.max(0.0) as i64;
    let x1 = ((x0 as f32 + w) as i64).min(fw as i64);
    let y1 = ((y0 as f32 + h) as i64).min(fh as i64);
    for y in y0..y1 {
        let row = y as usize * fw;
        for x in x0..x1 {
            let di = (row + x as usize) * 4;
            if di + 3 < frame.len() {
                frame[di] = color[2];
                frame[di + 1] = color[1];
                frame[di + 2] = color[0];
                frame[di + 3] = 255;
            }
        }
    }
}

/// 复古终端 CRT 后处理：在每个像素上叠加扫描线纹理与一条随时间（tick）向下移动的高亮扫描带，
/// 营造动态发光扫描线的复古显示器质感。扫描带位置由 tick 驱动，逐帧变化即产生「流动」动画。
fn apply_crt_effect(frame: &mut [u8], fw: usize, fh: usize, tick: u32) {
    let scan_period: usize = 3; // 每 3 行一条扫描暗线
    let scan_dark: f32 = 0.82; // 扫描暗线压暗系数
    let band_speed: f32 = 6.0; // 高亮扫描带每 tick 下移像素数
    let band_center = (tick as f32 * band_speed) % fh as f32;
    let band_width: f32 = 26.0; // 高亮扫描带半宽（高斯）
    let band_strength: f32 = 0.28; // 高亮扫描带增益
    for y in 0..fh {
        // 扫描线：每隔 scan_period 行整体压暗
        let scan = if y % scan_period == 0 { scan_dark } else { 1.0 };
        // 移动高亮带：以 band_center 为中心的高斯发光
        let dy = (y as f32 - band_center).abs();
        let glow = (-(dy * dy) / (2.0 * band_width * band_width)).exp() * band_strength;
        let factor = scan * (1.0 + glow);
        let row = y * fw;
        for x in 0..fw {
            let di = (row + x) * 4;
            frame[di] = (frame[di] as f32 * factor).clamp(0.0, 255.0) as u8;
            frame[di + 1] = (frame[di + 1] as f32 * factor).clamp(0.0, 255.0) as u8;
            frame[di + 2] = (frame[di + 2] as f32 * factor).clamp(0.0, 255.0) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_midi_loader::{ChunkedList, MidiDocument, NoteEvent, TrackManager};

    fn make_doc() -> MidiDocument {
        // 跨多个通道、在 tick=960 处全部处于发声状态的音符，
        // 让预览图里 CH01..CH06 与 ALL 行的键盘条同时点亮。
        let notes = vec![
            NoteEvent::new(0, 1920, 60, 100, 0), // CH1 C4
            NoteEvent::new(0, 1920, 64, 100, 0), // CH1 E4
            NoteEvent::new(0, 1920, 67, 100, 0), // CH1 G4
            NoteEvent::new(100, 1900, 55, 100, 1), // CH2 G3
            NoteEvent::new(100, 1900, 59, 100, 1), // CH2 B3
            NoteEvent::new(200, 1700, 72, 100, 2), // CH3 C5
            NoteEvent::new(300, 1600, 48, 100, 3), // CH4 C3
            NoteEvent::new(400, 1500, 76, 100, 4), // CH5 E5
            NoteEvent::new(500, 1400, 81, 100, 5), // CH6 A5
            NoteEvent::new(0, 1920, 72, 100, 7), // CH8 C5（验证中段通道）
            NoteEvent::new(0, 1920, 84, 100, 15), // CH16 C6（验证最高通道）
        ];
        let mut list: Vec<NoteEvent> = notes;
        list.sort_unstable_by_key(|n| n.start_tick);
        MidiDocument {
            next_note_id: 1,
            notes: vec![ChunkedList::from_sorted(list)],
            tempo_changes: vec![(0, 120.0)],
            time_signatures: vec![(0, 4, 4)],
            key_signatures: vec![(0, 0, false)],
            control_events: ChunkedList::new(),
            lyrics: vec![],
            markers: vec![],
            sys_ex: vec![],
            track_names: vec![Some("T1".into())],
            total_ticks: 1920,
            track_count: 1,
            tracks: TrackManager::new(1),
            division: 480,
            track_ports: vec![],
            track_max_end_ticks: vec![],
        }
    }

    #[test]
    fn test_render_produces_grid_cells() {
        let doc = make_doc();
        let cfg = MidiConsoleRenderConfig::from(&MidiConsoleConfig::default());
        let mut renderer = MidiConsoleRenderer::new(&doc, &cfg);
        let mut grid = vec![Cell::blank(); (COLS * ROWS) as usize];
        renderer.render(&mut grid, &doc, 240, 480, 60);

        // 应有大量非空格单元（键盘条 ▌ + 文本）
        let mut non_empty = 0usize;
        for c in &grid {
            if c.ch != ' ' {
                non_empty += 1;
            }
        }
        assert!(non_empty > 1000, "字符网格应产生大量非空格，实际 {non_empty}");
    }

    #[test]
    fn test_render_detects_active_channel_keys() {
        let doc = make_doc();
        let cfg = MidiConsoleRenderConfig::from(&MidiConsoleConfig::default());
        let mut renderer = MidiConsoleRenderer::new(&doc, &cfg);
        let mut grid = vec![Cell::blank(); (COLS * ROWS) as usize];
        // tick=240：验证跨通道独立检测（CH1 / CH2 / CH8 / CH16 各自按键）
        renderer.render(&mut grid, &doc, 240, 480, 60);
        assert!(renderer.pressed[0][60], "CH1 C4 应被按下");
        assert!(renderer.pressed[1][55], "CH2 G3 应被按下");
        assert!(renderer.pressed[7][72], "CH8 C5 应被按下（中段通道）");
        assert!(renderer.pressed[15][84], "CH16 C6 应被按下（最高通道）");
        assert!(renderer.note_count >= 4, "已计数的音符应 >= 4");
    }

    /// 渲染一帧并导出 PNG 预览图，供人工查看 MidiConsole 风格效果。
    #[test]
    fn test_render_preview_png() {
        use std::io::Write;

        let doc = make_doc();
        let cfg = MidiConsoleRenderConfig::from(&MidiConsoleConfig::default());
        let mut renderer = MidiConsoleRenderer::new(&doc, &cfg);

        // 整数倍 cell 比例（约 1:2，贴近真实终端），148×40 → 10×20 = 1480×800
        let cell = 10u32;
        let w = COLS * cell;
        let h = ROWS * (cell * 2);
        let mut frame = vec![0u8; (w * h * 4) as usize];
        // 取 tick=960（多个通道音符同时发声），键盘条应点亮为暖色
        render_midicomsole_frame(&mut renderer, &mut frame, w, h, &doc, 960, 480, 60);

        assert!(frame.iter().any(|&v| v != 0), "预览帧不应全黑");

        // ── 字形坐标正确性验证（闭环证据）──
        // 修复前 draw 回调的 px/py 被误当绝对坐标，所有字形堆在帧左上角 (0,0) 互相覆盖。
        // 修复后：表头文字应出现在正确格子，左上角第 0 列（header 行此处为空格）应保持背景。
        let cell_w_i = cell as usize;
        let cell_h_i = (cell * 2) as usize;
        let fw_us = w as usize;
        // (1) 左上角第 0 列不应被字形堆满
        let mut left_col_light = 0usize;
        for y in 0..cell_h_i {
            for x in 0..cell_w_i {
                let di = (y * fw_us + x) * 4;
                if frame[di] > 60 {
                    left_col_light += 1;
                }
            }
        }
        assert!(
            left_col_light < 50,
            "左上角第0列不应被字形堆满（坐标偏移修复后），实际亮像素 {left_col_light}"
        );
        // (2) 表头整行应有文字亮起（字形已正确定位）
        let mut header_light = 0usize;
        for y in 0..cell_h_i {
            for x in 0..fw_us {
                let di = (y * fw_us + x) * 4;
                if frame[di] > 60 {
                    header_light += 1;
                }
            }
        }
        assert!(
            header_light > 200,
            "表头行应有文字亮起（字形已正确定位），实际亮像素 {header_light}"
        );

        // BGRA -> RGBA 后写出 PNG
        let out_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target");
        let _ = std::fs::create_dir_all(&out_dir);
        let path = out_dir.join("midi_console_preview.png");
        let file = std::fs::File::create(&path).expect("创建预览 PNG 文件");
        let mut encoder = png::Encoder::new(file, w, h);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .expect("写入 PNG 头")
            .into_stream_writer()
            .expect("创建 PNG 流写入器");
        for pix in frame.chunks_exact(4) {
            let (b, g, r, a) = (pix[0], pix[1], pix[2], pix[3]);
            writer.write_all(&[r, g, b, a]).expect("写入 PNG 像素");
        }
        writer.finish().expect("结束 PNG 写入");

        // 以字符网格形式输出文本预览，便于无图环境核对布局
        let grid = {
            let mut g = vec![Cell::blank(); (COLS * ROWS) as usize];
            let mut r2 = MidiConsoleRenderer::new(&doc, &cfg);
            r2.render(&mut g, &doc, 960, 480, 60);
            g
        };
        println!("---- MidiConsole 字符网格预览 (148x40, . = 空格) ----");
        for r in 0..ROWS {
            let mut line = String::new();
            for c in 0..COLS {
                let ch = grid[cell_idx(r, c)].ch;
                line.push(if ch == ' ' { '.' } else { ch });
            }
            println!("{line}");
        }
        println!("------------------------------------------------------");
        println!("MidiConsole 预览图已写出: {}", path.display());
    }

    /// 按键亮度水平应随按下/松开平滑过渡（亮起渐变 + 熄灭渐变），而非瞬间跳变
    #[test]
    fn test_key_level_animates() {
        let doc = make_short_doc();
        let cfg = MidiConsoleRenderConfig {
            show_control_panel: true,
            keyboard_fade_frames: 30,
            control_fade_frames: 30,
            warm_key_color: [234, 234, 208],
        };
        let mut renderer = MidiConsoleRenderer::new(&doc, &cfg);
        let mut grid = vec![Cell::blank(); (COLS * ROWS) as usize];
        // 按住（tick=100，CH1 C4 发声中）：连续多帧后亮度应平滑趋近 1
        for _ in 0..30 {
            renderer.render(&mut grid, &doc, 100, 480, 60);
        }
        assert!(
            renderer.key_level[1][60] > 0.8,
            "按住时按键亮度应平滑趋近 1（亮起渐变动画），实际 {}",
            renderer.key_level[1][60]
        );
        // 松开（tick=300 > 结束 240）：连续多帧后亮度应淡出趋近 0
        for _ in 0..40 {
            renderer.render(&mut grid, &doc, 300, 480, 60);
        }
        assert!(
            renderer.key_level[1][60] < 0.1,
            "松开后按键亮度应淡出趋近 0（熄灭渐变动画），实际 {}",
            renderer.key_level[1][60]
        );
    }

    /// CRT 后处理应产生扫描线压暗，且随 tick 改变帧内容（动态发光扫描线）
    #[test]
    fn test_crt_scanline_darkens() {
        let w = 100u32;
        let h = 100u32;
        let mut frame = vec![0u8; (w * h * 4) as usize];
        for i in 0..frame.len() {
            frame[i] = if i % 4 == 3 { 255 } else { 200 };
        }
        let original = frame.clone();
        // tick=10 时移动亮带中心约在 y=60，远离顶行，便于比较扫描线压暗
        apply_crt_effect(&mut frame, w as usize, h as usize, 10);
        assert_ne!(frame, original, "CRT 后处理应改变像素（扫描线/亮带生效）");
        // 扫描线行（y%3==0，如 y=0）应比相邻非扫描线行（y=1）暗
        let r_scan = frame[0] as f32;
        let r_nonscan = frame[(1usize * w as usize) * 4] as f32;
        assert!(
            r_scan < r_nonscan,
            "扫描线行应比非扫描线行暗（r_scan={r_scan}, r_nonscan={r_nonscan}）"
        );
    }

    /// 仅含一个短音符（CH1 C4，tick 0..240）的文档，用于过渡动画测试
    fn make_short_doc() -> MidiDocument {
        let notes = vec![NoteEvent::new(0, 240, 60, 100, 0)];
        MidiDocument {
            next_note_id: 1,
            notes: vec![ChunkedList::from_sorted(notes)],
            tempo_changes: vec![(0, 120.0)],
            time_signatures: vec![(0, 4, 4)],
            key_signatures: vec![(0, 0, false)],
            control_events: ChunkedList::new(),
            lyrics: vec![],
            markers: vec![],
            sys_ex: vec![],
            track_names: vec![Some("T1".into())],
            total_ticks: 240,
            track_count: 1,
            tracks: TrackManager::new(1),
            division: 480,
            track_ports: vec![],
            track_max_end_ticks: vec![],
        }
    }
}
