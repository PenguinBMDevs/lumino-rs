//! RPN / NRPN 自动化 lane → 实时播放 CC 序列链路测试
//!
//! 覆盖两类缺陷（修复前必红）：
//! - T1：`AutomationTarget::Rpn/Nrpn` lane 被 `update_playback_notes` 静默丢弃；
//! - T2：同 tick 跨 lane 汇总按 lane 创建顺序重排，DataEntry(CC6) 可能先于
//!   选择 CC(101/100) 发出，导致数据写入上一次选择的参数。

use crate::editor::note::Note;
use crate::message::Message;
use crate::root::editor_ops::midi::tests::common::{attach_test_document, create_root};
use crate::toolbar;
use lumino_note_core::automation::{
    AutomationEvent, AutomationLane, AutomationTarget, SegmentShape,
};
use std::sync::{Arc, Mutex};

/// 记录型 MIDI 输出：按到达顺序保存全部 CC，用于顺序与字节断言。
struct RecordingOutput {
    /// `(channel, controller, value)` 到达顺序。
    cc_log: Arc<Mutex<Vec<(u8, u8, u8)>>>,
}

impl RecordingOutput {
    fn new() -> Self {
        Self {
            cc_log: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl lumino_midi_io::OutputConnection for RecordingOutput {
    fn note_on(&mut self, _ch: u8, _key: u8, _vel: u8) -> Result<(), lumino_midi_io::Error> {
        Ok(())
    }
    fn note_off(&mut self, _ch: u8, _key: u8, _vel: u8) -> Result<(), lumino_midi_io::Error> {
        Ok(())
    }
    fn control_change(
        &mut self,
        ch: u8,
        controller: u8,
        value: u8,
    ) -> Result<(), lumino_midi_io::Error> {
        if let Ok(mut log) = self.cc_log.lock() {
            log.push((ch, controller, value));
        }
        Ok(())
    }
    fn program_change(&mut self, _ch: u8, _program: u8) -> Result<(), lumino_midi_io::Error> {
        Ok(())
    }
    fn pitch_bend(&mut self, _ch: u8, _value: f32) -> Result<(), lumino_midi_io::Error> {
        Ok(())
    }
    fn channel_pressure(&mut self, _ch: u8, _pressure: u8) -> Result<(), lumino_midi_io::Error> {
        Ok(())
    }
    fn poly_pressure(
        &mut self,
        _ch: u8,
        _key: u8,
        _pressure: u8,
    ) -> Result<(), lumino_midi_io::Error> {
        Ok(())
    }
    fn send_raw(&mut self, _data: [u8; 3]) -> Result<(), lumino_midi_io::Error> {
        Ok(())
    }
    fn close(self: Box<Self>) {}
}

/// 只保留 RPN/NRPN 相关的控制字节，便于顺序断言。
fn rpn_cc_only(log: &[(u8, u8, u8)]) -> Vec<(u8, u8, u8)> {
    log.iter()
        .copied()
        .filter(|&(_, cc, _)| matches!(cc, 6 | 38 | 98 | 99 | 100 | 101))
        .collect()
}

/// T1：当前轨的 Rpn lane 必须展开为 CC 序列并到达输出。
#[test]
fn test_rpn_lane_reaches_midi_output() {
    let mut root = create_root();
    attach_test_document(&mut root);
    root.editor.editor_state.data.current_track = 0;
    root.editor.editor_state.data.insert_note(
        root.editor.editor_state.data.current_track,
        Note::new(0.0, 60, 480.0),
    );

    // RPN 1（微调）= 8500 → 期望 CC101=0, CC100=1, CC6=66, CC38=52
    root.editor
        .editor_state
        .data
        .automation_lanes
        .push(Arc::new(AutomationLane {
            target: AutomationTarget::Rpn { parameter: 1 },
            track: 0,
            channel: 0,
            events: vec![AutomationEvent::new(0, 8_500, SegmentShape::Step)],
        }));

    let output = RecordingOutput::new();
    let cc_log = Arc::clone(&output.cc_log);
    root.set_midi_output(Box::new(output));

    root.update(Message::Toolbar(toolbar::Event::Play));
    for _ in 0..200 {
        root.update_playback();
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    root.update(Message::Toolbar(toolbar::Event::Stop));

    let log = cc_log.lock().expect("cc log 未 poison");
    assert!(
        log.contains(&(0, 101, 0))
            && log.contains(&(0, 100, 1))
            && log.contains(&(0, 6, 66))
            && log.contains(&(0, 38, 52)),
        "Rpn{{1}} = 8500 应展开为 CC101/100 + CC6/38 并到达输出，实际 log={log:?}"
    );
    let related = rpn_cc_only(&log);
    let expected = vec![(0, 101, 0), (0, 100, 1), (0, 6, 66), (0, 38, 52)];
    assert_eq!(
        related, expected,
        "同一采样点内必须是 选择→数据 顺序，实际 {related:?}"
    );
}

/// T2：同一 tick 上 DataEntry 不得越过参数选择（跨 lane 顺序守卫）。
///
/// 构造真实场景：文件先出现 NRPN 序列（lane 创建顺序 99,98,6），
/// 之后在 tick 96 出现 RPN 序列（101,100,6）——
/// 按 lane 创建顺序会发出 6,101,100（错误），修复后必须为 101,100,6。
#[test]
fn test_same_tick_data_entry_never_precedes_selection() {
    use midly::loader::PackedControlEvent;

    let mut root = create_root();

    let control_events = lumino_midi_loader::ChunkedList::from_sorted(vec![
        // tick 0：NRPN 序列（建立 lane 顺序 99, 98, 6）
        PackedControlEvent::control_change(0, 0, 0, 99, 0),
        PackedControlEvent::control_change(0, 0, 0, 98, 99),
        PackedControlEvent::control_change(0, 0, 0, 6, 10),
        // tick 96：RPN 序列（101/100 的 lane 位于 CC6 lane 之后）
        PackedControlEvent::control_change(96, 0, 0, 101, 0),
        PackedControlEvent::control_change(96, 0, 0, 100, 1),
        PackedControlEvent::control_change(96, 0, 0, 6, 64),
    ]);

    let mut doc = crate::test_helpers::make_test_document();
    doc.notes = vec![
        lumino_midi_loader::ChunkedList::from_sorted(vec![lumino_midi_loader::NoteEvent {
            start_tick: 0,
            end_tick: 192,
            key: 60,
            velocity: 100,
            release_velocity: 0,
            channel: 0,
        }]),
        lumino_midi_loader::ChunkedList::new(),
    ];
    doc.control_events = control_events;
    doc.total_ticks = 192;

    root.set_midi_document(doc);
    let track0_notes = root
        .editor
        .editor_state
        .data
        .document
        .as_ref()
        .map(|d| d.get_track_notes(0))
        .unwrap_or_default();
    root.load_track_notes(0, &track0_notes);
    assert_eq!(root.editor.editor_state.data.current_track, 0);

    let output = RecordingOutput::new();
    let cc_log = Arc::clone(&output.cc_log);
    root.set_midi_output(Box::new(output));

    root.update(Message::Toolbar(toolbar::Event::Play));
    for _ in 0..200 {
        root.update_playback();
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    root.update(Message::Toolbar(toolbar::Event::Stop));

    let log = cc_log.lock().expect("cc log 未 poison");
    let related = rpn_cc_only(&log);
    let expected = vec![
        (0, 99, 0),
        (0, 98, 99),
        (0, 6, 10),
        (0, 101, 0),
        (0, 100, 1),
        (0, 6, 64),
    ];
    assert_eq!(
        related, expected,
        "tick 96 的 RPN 序列必须为 选择(101,100) 先于数据(6)，实际 {related:?}"
    );
}
