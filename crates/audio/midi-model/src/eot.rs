//! EOT / 悬挂音符的加载期诊断与裁剪（midly `TrackTiming` 的下游消费）。
//!
//! 解析器（midly）在兼容模式下保留 EOT 之后的事件，并给出逐轨 timing：
//! `eot_tick` / `physical_end_tick` / `content_end_tick` / `has_events_after_eot` /
//! `has_unclosed_notes`。本模块把这些信息整理成加载期报告，并提供**显式**裁剪能力。
//!
//! 产品策略：加载**不静默丢数据**；是否提示/裁剪由 UI 决定。裁剪只动有异常的轨。

use midly::loader::TrackTiming;

use crate::document::MidiDocument;

/// 加载期逐轨 timing 报告（索引 = track index）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadTimingReport {
    tracks: Vec<TrackTiming>,
}

impl LoadTimingReport {
    pub(crate) fn from_tracks(tracks: Vec<TrackTiming>) -> Self {
        Self { tracks }
    }

    /// 逐轨 timing 切片（索引 = track index）。
    pub fn tracks(&self) -> &[TrackTiming] {
        &self.tracks
    }

    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    pub fn track(&self, index: usize) -> Option<TrackTiming> {
        self.tracks.get(index).copied()
    }

    /// 在首个 EOT 之后仍有事件的轨数（畸形尾部）。
    pub fn tracks_with_events_after_eot(&self) -> usize {
        self.tracks
            .iter()
            .filter(|t| t.has_events_after_eot)
            .count()
    }

    /// 结束时仍有未 NoteOff 音符的轨数（悬挂音符）。
    pub fn tracks_with_unclosed_notes(&self) -> usize {
        self.tracks.iter().filter(|t| t.has_unclosed_notes).count()
    }

    /// 缺少 EOT 的轨数（导出前提示用）。
    pub fn tracks_missing_eot(&self) -> usize {
        self.tracks.iter().filter(|t| t.eot_tick.is_none()).count()
    }

    /// 任一诊断（EOT 后事件 / 悬挂音符）命中的轨数。
    pub fn tracks_with_diagnostics(&self) -> usize {
        self.tracks
            .iter()
            .filter(|t| t.has_events_after_eot || t.has_unclosed_notes)
            .count()
    }

    /// 所有轨都不含「EOT 之后的事件」（悬挂音符不计入 Strict）。
    pub fn is_strict_clean(&self) -> bool {
        self.tracks.iter().all(TrackTiming::is_strict_clean)
    }

    /// 全文件有效结尾 = 各轨**声明结尾的最大值**。
    ///
    /// 产品原则：**EOT 本身不得导致事件解析被硬截断**。
    /// 取 max 而不是「较早/最后一个不越界轨的 EOT」：若 A 的 EOT(1000) 晚于
    /// B 的 EOT(500)，A 在 500..1000 之间可能有合法事件；取 500 会误裁，因此必须取 1000。
    pub fn declared_end_tick(&self) -> u32 {
        self.tracks
            .iter()
            .map(TrackTiming::declared_end_tick)
            .max()
            .unwrap_or(0)
    }
}

/// 裁剪结果。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TrimReport {
    pub tracks_touched: usize,
    pub notes_removed: u64,
    pub controls_removed: u64,
    pub text_removed: u64,
    pub sysex_removed: u64,
    pub total_ticks_before: u32,
    pub total_ticks_after: u32,
}

impl TrimReport {
    /// 是否真的删掉了东西。
    pub fn is_noop(&self) -> bool {
        self.notes_removed == 0
            && self.controls_removed == 0
            && self.text_removed == 0
            && self.sysex_removed == 0
    }
}

impl MidiDocument {
    /// 按加载报告裁剪「首个 EOT 之后的事件」，返回删除计数。
    ///
    /// 语义：
    /// - 只处理 `has_events_after_eot` 的轨，阈值取**该轨自己的**
    ///   [`TrackTiming::declared_end_tick`]；
    /// - **绝不**用「较早轨的 EOT」做全局截断（会把晚 EOT 轨的合法内容一起删掉）；
    /// - 音符：`start_tick > 阈值` 的整条删除（悬挂音符不截断——那属于另一个策略）；
    /// - 控制 / 歌词 / 标记 / 文本 / SysEx：`tick > 阈值` 的删除；
    /// - tempo / 拍号 / 调号是文件级语义，不动；
    /// - 裁剪后重算 `total_ticks` 并弃用 `max_end` 缓存。
    ///
    /// 这是**显式**操作（清理畸形文件），加载路径不会自动调用——
    /// 解析阶段永远不因 EOT 丢弃任何事件。
    pub fn trim_after_eot(&mut self, report: &LoadTimingReport) -> TrimReport {
        let ends: Vec<Option<u32>> = (0..self.track_count as usize)
            .map(|i| {
                report
                    .track(i)
                    .filter(|t| t.has_events_after_eot)
                    .map(|t| t.declared_end_tick())
            })
            .collect();

        let mut out = TrimReport {
            total_ticks_before: self.total_ticks,
            ..TrimReport::default()
        };

        // 音符：只重建有异常的轨（过滤保持 start_tick 升序）。
        for (idx, end) in ends.iter().enumerate() {
            let Some(end) = *end else { continue };
            let Some(list) = self.notes.get(idx) else {
                continue;
            };
            let before = list.len();
            if before == 0 {
                continue;
            }
            let filtered = crate::chunked_list::ChunkedList::from_sorted_iter(
                list.iter().filter(|n| n.start_tick <= end).copied(),
            );
            let after = filtered.len();
            if after != before {
                self.notes[idx] = filtered;
                out.notes_removed += (before - after) as u64;
                out.tracks_touched += 1;
            }
        }

        // 控制事件（全局列表，按事件所属轨的阈值过滤）。
        let ctrl_before = self.control_events.len();
        let ctrl = crate::chunked_list::ChunkedList::from_sorted(
            self.control_events
                .iter()
                .filter(|e| {
                    ends.get(e.track as usize)
                        .copied()
                        .flatten()
                        .is_none_or(|end| e.tick <= end)
                })
                .copied()
                .collect::<Vec<_>>(),
        );
        out.controls_removed = (ctrl_before - ctrl.len()) as u64;
        if out.controls_removed > 0 {
            self.control_events = ctrl;
        }

        // 文本类事件：Vec<(tick, track, ..)>。
        let keep_text = |tick: u32, track: u16, ends: &[Option<u32>]| -> bool {
            ends.get(track as usize)
                .copied()
                .flatten()
                .is_none_or(|end| tick <= end)
        };
        let before = self.lyrics.len();
        self.lyrics.retain(|e| keep_text(e.0, e.1, &ends));
        out.text_removed += (before - self.lyrics.len()) as u64;
        let before = self.markers.len();
        self.markers.retain(|e| keep_text(e.0, e.1, &ends));
        out.text_removed += (before - self.markers.len()) as u64;
        let before = self.text_events.len();
        self.text_events.retain(|e| keep_text(e.0, e.1, &ends));
        out.text_removed += (before - self.text_events.len()) as u64;
        let before = self.sys_ex.len();
        self.sys_ex.retain(|e| keep_text(e.0, e.1, &ends));
        out.sysex_removed += (before - self.sys_ex.len()) as u64;

        // 重算总长：与加载语义一致——取「声明结尾（EOT）」与残留内容末尾的较大者。
        // （残留音符可能跨过 EOT 开始、结束在其后；不能让它们落到时间线之外。）
        let max_note_end = self
            .notes
            .iter()
            .flat_map(|list| list.iter())
            .map(|n| n.end_tick)
            .max()
            .unwrap_or(0);
        let max_ctrl = self.control_events.last().map(|e| e.tick).unwrap_or(0);
        self.total_ticks = max_note_end.max(max_ctrl).max(report.declared_end_tick());
        out.total_ticks_after = self.total_ticks;

        // max_end 缓存全部置脏（裁剪只发生在异常轨，但缓存粒度是全局列表）。
        for cell in &self.track_max_end_ticks {
            if let Ok(mut slot) = cell.lock() {
                *slot = None;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_vlq(out: &mut Vec<u8>, mut value: u32) {
        let mut buf = [0u8; 5];
        let mut n = 0;
        loop {
            buf[n] = (value & 0x7F) as u8;
            value >>= 7;
            n += 1;
            if value == 0 {
                break;
            }
        }
        for i in (0..n).rev() {
            let mut byte = buf[i];
            if i != 0 {
                byte |= 0x80;
            }
            out.push(byte);
        }
    }

    fn smf_tracks(bodies: &[&[u8]]) -> Vec<u8> {
        let format: u16 = if bodies.len() == 1 { 0 } else { 1 };
        let mut out = Vec::new();
        out.extend_from_slice(b"MThd");
        out.extend_from_slice(&6u32.to_be_bytes());
        out.extend_from_slice(&format.to_be_bytes());
        out.extend_from_slice(&(bodies.len() as u16).to_be_bytes());
        out.extend_from_slice(&480u16.to_be_bytes());
        for body in bodies {
            out.extend_from_slice(b"MTrk");
            out.extend_from_slice(&(body.len() as u32).to_be_bytes());
            out.extend_from_slice(body);
        }
        out
    }

    fn smf_one_track(body: &[u8]) -> Vec<u8> {
        smf_tracks(&[body])
    }

    fn ev(body: &mut Vec<u8>, delta: u32, bytes: &[u8]) {
        push_vlq(body, delta);
        body.extend_from_slice(bytes);
    }

    /// EOT(100) 之后仍有音符事件（无尾部 EOT）：报告标记，裁剪只删 EOT 后音符。
    #[test]
    fn trim_removes_events_after_eot_and_reports() {
        let mut body = Vec::new();
        ev(&mut body, 0, &[0x90, 60, 100]);
        ev(&mut body, 100, &[0x80, 60, 0]);
        ev(&mut body, 0, &[0xFF, 0x2F, 0x00]); // EOT @100
        ev(&mut body, 100, &[0x90, 62, 90]); // 畸形：EOT 之后
        ev(&mut body, 100, &[0x80, 62, 0]); // @300（物理末尾）
        let bytes = smf_one_track(&body);

        let (mut doc, _, _notes, report) =
            MidiDocument::from_notes_bytes_with_timing(&bytes, None).expect("加载失败");

        assert_eq!(report.len(), 1);
        assert_eq!(report.tracks_with_events_after_eot(), 1);
        assert!(!report.is_strict_clean());
        let timing = report.track(0).expect("track 0");
        assert_eq!(timing.eot_tick, Some(100));
        assert_eq!(timing.declared_end_tick(), 100);
        assert_eq!(timing.physical_end_tick, 300);
        assert_eq!(doc.notes[0].len(), 2, "兼容模式保留 EOT 后事件");
        assert_eq!(
            doc.total_ticks, 100,
            "时间线取声明结尾（EOT=100），畸形尾部不拉长时长"
        );

        let trimmed = doc.trim_after_eot(&report);
        assert_eq!(trimmed.notes_removed, 1);
        assert_eq!(trimmed.total_ticks_before, 100);
        assert_eq!(trimmed.total_ticks_after, 100);
        assert_eq!(doc.notes[0].len(), 1);
        assert_eq!(doc.total_ticks, 100);
    }

    /// 干净文件：报告无诊断，裁剪是 no-op。
    #[test]
    fn clean_file_is_noop() {
        let mut body = Vec::new();
        ev(&mut body, 0, &[0x90, 60, 100]);
        ev(&mut body, 100, &[0x80, 60, 0]);
        ev(&mut body, 0, &[0xFF, 0x2F, 0x00]);
        let bytes = smf_one_track(&body);

        let (mut doc, _, _, report) =
            MidiDocument::from_notes_bytes_with_timing(&bytes, None).expect("加载失败");
        assert_eq!(report.tracks_with_diagnostics(), 0);
        assert!(report.is_strict_clean());

        let trimmed = doc.trim_after_eot(&report);
        assert!(trimmed.is_noop());
        assert_eq!(trimmed.total_ticks_before, doc.total_ticks);
    }

    /// 悬挂音符：被标记，但不属于 EOT 后事件（裁不裁由产品决定，本函数不动）。
    #[test]
    fn unclosed_note_is_reported_but_not_trimmed() {
        let mut body = Vec::new();
        ev(&mut body, 0, &[0x90, 60, 100]); // 不 NoteOff
        ev(&mut body, 480, &[0xFF, 0x2F, 0x00]);
        let bytes = smf_one_track(&body);

        let (mut doc, _, _, report) =
            MidiDocument::from_notes_bytes_with_timing(&bytes, None).expect("加载失败");
        assert_eq!(report.tracks_with_unclosed_notes(), 1);
        assert_eq!(report.tracks_with_events_after_eot(), 0);
        let before = doc.notes[0].len();
        let trimmed = doc.trim_after_eot(&report);
        assert!(trimmed.is_noop());
        assert_eq!(doc.notes[0].len(), before);
    }

    /// 严格模式：EOT 后仍有事件 → `EotViolation`；干净文件正常返回。
    #[test]
    fn strict_rejects_events_after_eot_only() {
        // 畸形：EOT 后仍有音符。
        let mut body = Vec::new();
        ev(&mut body, 0, &[0x90, 60, 100]);
        ev(&mut body, 100, &[0x80, 60, 0]);
        ev(&mut body, 0, &[0xFF, 0x2F, 0x00]);
        ev(&mut body, 100, &[0x90, 62, 90]);
        ev(&mut body, 100, &[0x80, 62, 0]);
        let bytes = smf_one_track(&body);
        match MidiDocument::from_notes_bytes_strict(&bytes, None) {
            Err(crate::error::LoaderError::EotViolation { tracks }) => assert_eq!(tracks, 1),
            other => panic!(
                "期望 EotViolation，实得 {other:?}",
                other = other.map(|_| ())
            ),
        }

        // 干净文件：严格模式应成功。
        let mut ok = Vec::new();
        ev(&mut ok, 0, &[0x90, 60, 100]);
        ev(&mut ok, 100, &[0x80, 60, 0]);
        ev(&mut ok, 0, &[0xFF, 0x2F, 0x00]);
        assert!(MidiDocument::from_notes_bytes_strict(&smf_one_track(&ok), None).is_ok());
    }

    /// 缺 EOT 的轨会被计数（导出前提示的输入）。
    #[test]
    fn missing_eot_is_counted() {
        let mut body = Vec::new();
        ev(&mut body, 0, &[0x90, 60, 100]);
        ev(&mut body, 100, &[0x80, 60, 0]); // 没有 EOT
        let bytes = smf_one_track(&body);
        let (_, _, _, report) =
            MidiDocument::from_notes_bytes_with_timing(&bytes, None).expect("加载失败");
        assert_eq!(report.tracks_missing_eot(), 1);
        // 缺 EOT 不属于 Strict 违规（Strict 只看 EOT 后事件）。
        assert!(report.is_strict_clean());
        assert!(!report.is_empty());
    }

    /// **产品原则**：EOT 本身不得导致事件被硬截断。
    ///
    /// A 的 EOT(1000) 晚于 B(500)：全文件有效结尾必须取 **1000**（max），
    /// 而不是 B 的 500——否则 A 在 500..1000 的合法内容会被误裁。
    /// 裁剪只允许删「各自轨 EOT 之后」的事件。
    #[test]
    fn latest_eot_wins_and_never_cuts_earlier_content() {
        // 轨 A：500–900 合法音符 → EOT@1000 → 1200/1300 垃圾
        let mut a = Vec::new();
        ev(&mut a, 500, &[0x90, 60, 100]);
        ev(&mut a, 400, &[0x80, 60, 0]); // tick 900
        ev(&mut a, 100, &[0xFF, 0x2F, 0x00]); // EOT @1000
        ev(&mut a, 200, &[0x90, 62, 90]); // tick 1200（EOT 后）
        ev(&mut a, 100, &[0x80, 62, 0]); // tick 1300
        // 轨 B：0–400 → EOT@500
        let mut b = Vec::new();
        ev(&mut b, 0, &[0x90, 64, 100]);
        ev(&mut b, 400, &[0x80, 64, 0]); // tick 400
        ev(&mut b, 100, &[0xFF, 0x2F, 0x00]); // EOT @500
        let bytes = smf_tracks(&[&a, &b]);

        let (mut doc, _, _, report) =
            MidiDocument::from_notes_bytes_with_timing(&bytes, None).expect("加载失败");
        assert_eq!(
            report.declared_end_tick(),
            1000,
            "必须取最晚 EOT（1000），而不是较早的 B=500"
        );
        assert_eq!(
            doc.total_ticks, 1000,
            "时间线取最晚 EOT，不被 1200/1300 的畸形尾部拉长"
        );
        assert_eq!(report.tracks_with_events_after_eot(), 1);

        let trimmed = doc.trim_after_eot(&report);
        assert_eq!(trimmed.notes_removed, 1, "只删 A 的 EOT 后事件");
        let starts: Vec<u32> = doc.notes[0].iter().map(|n| n.start_tick).collect();
        assert!(
            starts.contains(&500),
            "A 的合法内容（500 起）不得被截断：{starts:?}"
        );
        assert!(
            !starts.contains(&1200),
            "EOT 之后的事件应被裁掉：{starts:?}"
        );
        assert_eq!(doc.notes[1].len(), 1, "B 不受影响");
    }
}
