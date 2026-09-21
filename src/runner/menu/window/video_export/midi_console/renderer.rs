use lumino_midi_loader::MidiDocument;

use super::cpu::{cc_field_index, format_field, key_color, play_speed, ramp, set_cell, set_text};
use super::*;

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
    pub fn render(
        &mut self,
        grid: &mut [Cell],
        document: &MidiDocument,
        tick: u32,
        ppq: u32,
        fps: u32,
    ) {
        self.advance(document, tick);
        let gw = COLS as usize;
        for c in grid.iter_mut() {
            *c = Cell::blank();
        }

        self.draw_header(grid, document, tick, ppq, fps);
        self.draw_stats(grid, document, tick, ppq, fps);
        self.draw_control_header(grid);

        for ch in 0..16usize {
            // 键盘条 + 控制数据在同一行：键盘在左（cols 5..68），数据在右（col 71+），横向对齐
            let kb_row = 3 + ch * 2;
            self.draw_keyboard_row(grid, kb_row as u32, ch);
            if self.config.show_control_panel {
                self.draw_control_row(grid, kb_row as u32, ch);
            }
        }
        // ALL 合并行
        self.draw_keyboard_row(grid, (3 + 16 * 2) as u32, 16);
        let _ = gw;
    }

    fn draw_header(
        &self,
        grid: &mut [Cell],
        _document: &MidiDocument,
        _tick: u32,
        _ppq: u32,
        _fps: u32,
    ) {
        set_text(grid, 0, 1, "LUMINO MIDICONSOLE", [220, 220, 230], BG);
        // 显式标注：CH01..CH16 行即 MIDI 通道 1..16（非轨道），按键亮起按通道独立检测。
        // 仅用 ASCII（Consolas/DejaVu 保证有字形），避免 ▶/·/中文 等缺失字形导致空白。
        set_text(grid, 0, 20, "- CH1-16 = MIDI CHANNEL", [150, 170, 210], BG);
        set_text(grid, 0, COLS - 10, "> PLAYING", [120, 220, 120], BG);
    }

    fn draw_stats(
        &self,
        grid: &mut [Cell],
        document: &MidiDocument,
        tick: u32,
        ppq: u32,
        fps: u32,
    ) {
        let bpm = super::counter_stats::current_bpm(&document.tempo_changes, tick);
        let (num, den) =
            super::counter_stats::current_time_signature(&document.time_signatures, tick);
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
            "PC", "VOL", "EXP", "PAN", "P.BEND", "P.RANGE", "MOD", "HOLD", "CUT", "RESO", "ATT",
            "DEC", "REL",
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
