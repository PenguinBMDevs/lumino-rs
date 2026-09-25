use super::*;

impl Editor {
    /// 构建 Lumino 私有 JSON 剪贴板文本（跨平台退化路径）。
    ///
    /// 两遍扫描选中音符：第一遍算 origin，第二遍流式拼 JSON，不物化 `Vec<Value>`。
    pub(super) fn build_clipboard_json(&self, track: usize, division: u16) -> Option<String> {
        let mut min_tick = f32::INFINITY;
        let mut min_key = u16::MAX;
        self.each_selected_note_on_current_track(|n| {
            let t = n.start_tick as f32;
            if t < min_tick {
                min_tick = t;
            }
            let k = n.key as u16;
            if k < min_key {
                min_key = k;
            }
        });
        if min_tick.is_infinite() {
            return None;
        }
        let origin_tick = if min_tick.is_finite() { min_tick } else { 0.0 };
        let origin_key = if min_key != u16::MAX { min_key } else { 0 };

        let mut s = String::with_capacity(2048);
        use std::fmt::Write as _;
        let _ = write!(
            s,
            "{{\"lumino\":\"{}\",\"version\":{},\"track\":{},\"origin_tick\":{},\"origin_key\":{},\"division\":{},\"notes\":[",
            CLIPBOARD_FORMAT, CLIPBOARD_VERSION, track, origin_tick, origin_key, division
        );
        let mut first = true;
        self.each_selected_note_on_current_track(|n| {
            let tick = (n.start_tick as f32 - origin_tick).max(0.0);
            let key = (n.key as i32 - origin_key as i32).max(0) as u16;
            let length = (n.end_tick - n.start_tick) as f32;
            if !first {
                s.push(',');
            }
            first = false;
            let _ = write!(
                s,
                "{{\"tick\":{},\"key\":{},\"length\":{},\"velocity\":{},\"channel\":{}}}",
                tick, key, length, n.velocity, n.channel
            );
        });
        s.push_str("]}");
        Some(s)
    }

    /// 构建紧凑二进制剪贴板载荷（Windows 二进制路径）。
    ///
    /// **流式、零大数组**：第一遍扫描选中音符算 origin（min tick/key），第二遍按文档
    /// tick 顺序 `filter_map` 出 `ClipRecord` 直接喂给 `encode_clipboard`，不物化任何
    /// `Vec<NoteEvent>` / `Vec<ClipRecord>`，故「全选」10M 音符也只占用约 67MB 载荷内存。
    ///
    /// 2026-09 全选快路径：`选中数 == 轨道音符数` 时跳过逐音符哈希命中判断与随机
    /// `get`（百万级全选复制免 2×N 次哈希 + 缓存未命中），两遍均顺序扫描。
    ///
    /// `pub`：基准（benches）与外部调用方复用生产编码路径，避免实现漂移。
    pub fn build_clipboard_binary(&self, track: usize, division: u16) -> Option<Vec<u8>> {
        let interaction = &self.editor_state.interaction;
        let notes = self.editor_state.data.current_track_notes();
        let selected = &interaction.selected_notes;
        let full = !notes.is_empty() && selected.len() == notes.len();

        // 第一遍：origin + count
        // 非全选也走「轨道顺序扫描 + 位图命中」：`SelectionSet` 现为位图（O(1) 命中、
        // 顺序读），避免逐索引随机 `notes.get` 与哈希退化（曾实测 19.2M 轨道 3M 选中
        // 复制 160s）。两遍均为顺序读，缓存友好。
        let mut min_tick = u32::MAX;
        let mut min_key = u8::MAX;
        let count = if full {
            for n in notes.iter() {
                min_tick = min_tick.min(n.start_tick);
                min_key = min_key.min(n.key);
            }
            notes.len()
        } else {
            let mut c = 0usize;
            for (i, n) in notes.iter().enumerate() {
                if selected.contains(&i) {
                    min_tick = min_tick.min(n.start_tick);
                    min_key = min_key.min(n.key);
                    c += 1;
                }
            }
            c
        };
        if count == 0 {
            return None;
        }
        let origin_tick = if min_tick != u32::MAX { min_tick } else { 0 };
        let origin_key = if min_key != u8::MAX { min_key } else { 0 };

        let make = |n: &lumino_midi_loader::NoteEvent| {
            ClipRecord::new(
                n.start_tick - origin_tick,
                n.end_tick - n.start_tick,
                (n.key as i32 - origin_key as i32).max(0) as u8,
                n.velocity,
                n.channel,
                track as u16,
            )
        };

        // 第二遍：流式编码（文档顺序即 tick 升序；delta 编码使密集排布极省）
        let bytes = if full {
            encode_clipboard(
                notes.iter().map(make),
                count,
                division,
                origin_tick,
                origin_key,
                track as u16,
            )
        } else {
            encode_clipboard(
                notes
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| selected.contains(i))
                    .map(|(_, n)| make(n)),
                count,
                division,
                origin_tick,
                origin_key,
                track as u16,
            )
        };
        Some(bytes)
    }

    /// 构建 Domino 互通剪贴板载荷（Windows 二进制路径的并行格式）。
    ///
    /// 把选中音符收集为 `NoteEvent`（key/velocity/channel 绝对，tick 按 Domino 的
    /// division=480 网格重采样），再交给 `encode_domino_clipboard` 编码为
    /// `PortalSequenceData` + zlib，供 Domino 直接粘贴。
    ///
    /// **交互保护上限**：选中数超过 [`DOMINO_MAX_NOTES`] 时返回 `None`（跳过该格式）。
    /// 百万级选中编码 ~100MB 原始体 + zlib 是复制路径的秒级成本，而 Domino 实际
    /// 粘贴场景不可能承载百万级音符；Lumino 二进制主格式不受影响。
    #[cfg(windows)]
    pub(super) fn build_clipboard_domino(&self) -> Option<Vec<u8>> {
        let selected_len = self.editor_state.interaction.selected_notes.len();
        if selected_len > DOMINO_MAX_NOTES {
            tracing::debug!(
                "Editor: 选中 {} 音符超过 Domino 互通上限 {}，跳过 Domino 格式（仅写 Lumino 二进制）",
                selected_len,
                DOMINO_MAX_NOTES
            );
            return None;
        }
        let target_div = self
            .editor_state
            .data
            .document
            .as_ref()
            .map(|d| d.division)
            .unwrap_or(480);
        // Lumino 文档以 target_div 表达 tick，需换算到 Domino 的 480 网格
        let ratio = if target_div != 480 {
            480.0 / target_div as f64
        } else {
            1.0
        };
        let mut events: Vec<lumino_midi_model::note_event::NoteEvent> = Vec::new();
        self.each_selected_note_on_current_track(|n| {
            let start = if ratio == 1.0 {
                n.start_tick
            } else {
                (n.start_tick as f64 * ratio).round() as u32
            };
            let end = if ratio == 1.0 {
                n.end_tick
            } else {
                (n.end_tick as f64 * ratio).round() as u32
            };
            events.push(lumino_midi_model::note_event::NoteEvent::new(
                start,
                end,
                (n.key as u32).min(127) as u8,
                n.velocity,
                n.channel,
            ));
        });
        if events.is_empty() {
            return None;
        }
        encode_domino_clipboard(&events).ok()
    }
}
