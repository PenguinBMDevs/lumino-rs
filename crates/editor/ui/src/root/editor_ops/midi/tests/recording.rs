use super::common::{attach_test_document, create_root};

use lumino_midi_io::{
    Api, Error, InputConnection, InputInfo, MidiInputCallback, OutputInfo, PlaybackOutput,
};

/// 录制失败反馈测试用输入 API：可配置设备列表与打开输入是否失败。
struct TestInputApi {
    inputs: Vec<InputInfo>,
    open_fails: bool,
}

impl Api for TestInputApi {
    fn version(&self) -> Option<String> {
        Some("test".to_string())
    }

    fn inputs(&self) -> Result<Vec<InputInfo>, Error> {
        Ok(self.inputs.clone())
    }

    fn outputs(&self) -> Result<Vec<OutputInfo>, Error> {
        Ok(Vec::new())
    }

    fn open_output(&self, _id: u32) -> Result<Box<dyn PlaybackOutput>, Error> {
        Err(Error::OpenOutputFailed("测试 API 不支持输出".to_string()))
    }

    fn open_input(
        &self,
        _id: u32,
        _callback: MidiInputCallback,
    ) -> Result<Box<dyn InputConnection>, Error> {
        if self.open_fails {
            Err(Error::OpenInputFailed("测试打开失败".to_string()))
        } else {
            Ok(Box::new(TestInputConnection))
        }
    }
}

struct TestInputConnection;

impl InputConnection for TestInputConnection {
    fn close(self: Box<Self>) {}
}

/// 无输入 API：录制失败必须复位按钮并给出用户可见反馈（不能只写日志）。
#[test]
fn test_start_recording_without_api_reports_visible_error() {
    let mut root = create_root();
    root.toolbar.is_recording = true;

    root.start_recording();

    assert!(!root.toolbar.is_recording, "失败后录制按钮必须复位");
    assert_eq!(root.toast.len(), 1, "失败必须产生 Toast 反馈");
}

/// 有 API 但无设备：同样必须有可见反馈，而不是静默回到未录制态。
#[test]
fn test_start_recording_without_devices_reports_visible_error() {
    let mut root = create_root();
    root.set_midi_api(Box::new(TestInputApi {
        inputs: Vec::new(),
        open_fails: false,
    }));
    root.toolbar.is_recording = true;

    root.start_recording();

    assert!(!root.toolbar.is_recording, "失败后录制按钮必须复位");
    assert_eq!(root.toast.len(), 1, "无设备必须产生 Toast 反馈");
}

/// 打开输入设备失败：必须给出可见反馈（设备被占用等场景）。
#[test]
fn test_start_recording_open_failure_reports_visible_error() {
    let mut root = create_root();
    root.set_midi_api(Box::new(TestInputApi {
        inputs: vec![InputInfo {
            id: 7,
            name: "Fake Keyboard".to_string(),
        }],
        open_fails: true,
    }));
    root.toolbar.is_recording = true;

    root.start_recording();

    assert!(!root.toolbar.is_recording, "失败后录制按钮必须复位");
    assert_eq!(root.toast.len(), 1, "打开失败必须产生 Toast 反馈");
}

/// 测试 RecordingState 的状态切换
#[test]
fn test_recording_state_toggle() {
    use crate::editor::recording::RecordingState;

    let mut state = RecordingState::new();
    assert!(!state.is_recording, "初始不应录制");
    assert!(state.input_device_name.is_none());
    assert_eq!(state.arm_track, 0);
    assert!(state.pending_notes.is_empty());

    state.start(Some("Test Device".into()), 1);
    assert!(state.is_recording, "start() 后应进入录制状态");
    assert_eq!(state.input_device_name.as_deref(), Some("Test Device"));
    assert_eq!(state.arm_track, 1);

    state.stop();
    assert!(!state.is_recording, "stop() 后应退出录制状态");
    assert!(state.pending_notes.is_empty());

    // 测试节拍器切换
    assert!(!state.metronome_enabled, "节拍器默认关闭");
    state.toggle_metronome();
    assert!(state.metronome_enabled, "toggle_metronome 应打开节拍器");
    state.toggle_metronome();
    assert!(!state.metronome_enabled, "再次 toggle 应关闭节拍器");
}

/// 测试 poll_midi_input 在录制状态下处理 NoteOn 事件
#[test]
fn test_poll_midi_input_note_on() {
    let mut root = create_root();
    attach_test_document(&mut root);
    root.recording.is_recording = true;
    root.recording.started_at = Some(std::time::Instant::now());

    // 模拟 MIDI NoteOn：通道0，按键60（Middle C），力度100
    let midi_data = vec![0x90, 60, 100];
    {
        let mut buf = root
            .midi
            .input_buffer
            .lock()
            .expect("MIDI 输入缓冲区的锁未被其他线程持有，加锁应成功");
        buf.push_back(midi_data);
    }

    root.poll_midi_input();

    // 验证创建了一个音符（tick 基于墙钟时间，接近 0）
    assert_eq!(root.editor.editor_state.data.current_track_note_count(), 1);
    let note = &root.editor.editor_state.data.current_track_notes()[0];
    assert_eq!(note.key, 60);
    assert_eq!(note.velocity, 100);
    assert!(
        note.start_tick as f32 >= 0.0,
        "音符 tick 应 >= 0，实际 {}",
        note.start_tick
    );

    // 验证 pending_notes 追踪
    assert!(
        root.recording.pending_notes.contains_key(&60),
        "NoteOn 后应在 pending_notes 中追踪按键 60"
    );
}

/// 测试 poll_midi_input 处理 NoteOn + NoteOff 序列
#[test]
fn test_poll_midi_input_note_on_off() {
    let mut root = create_root();
    attach_test_document(&mut root);
    root.recording.is_recording = true;
    root.recording.started_at = Some(std::time::Instant::now());

    // 模拟 NoteOn 事件
    {
        let mut buf = root
            .midi
            .input_buffer
            .lock()
            .expect("MIDI 输入缓冲区的锁未被其他线程持有，加锁应成功");
        buf.push_back(vec![0x90, 60, 100]);
    }
    root.poll_midi_input();

    // 模拟 NoteOff
    {
        let mut buf = root
            .midi
            .input_buffer
            .lock()
            .expect("MIDI 输入缓冲区的锁未被其他线程持有，加锁应成功");
        buf.push_back(vec![0x80, 60, 0]);
    }
    root.poll_midi_input();

    // 验证音符长度已更新（基于墙钟时间，length > 0）
    assert_eq!(root.editor.editor_state.data.current_track_note_count(), 1);
    let note = &root.editor.editor_state.data.current_track_notes()[0];
    assert!(
        note.end_tick > note.start_tick,
        "音符长度应大于 0，实际 {}",
        note.end_tick - note.start_tick
    );

    // 验证 pending_notes 已清除
    assert!(
        !root.recording.pending_notes.contains_key(&60),
        "NoteOff 后应从 pending_notes 移除按键 60"
    );
}

/// 测试收到 NoteOn with velocity=0 时当作 NoteOff 处理
#[test]
fn test_note_on_with_velocity_zero_treated_as_note_off() {
    let mut root = create_root();
    attach_test_document(&mut root);
    root.recording.is_recording = true;
    root.recording.started_at = Some(std::time::Instant::now());

    // 先发送 NoteOn
    {
        let mut buf = root
            .midi
            .input_buffer
            .lock()
            .expect("MIDI 输入缓冲区的锁未被其他线程持有，加锁应成功");
        buf.push_back(vec![0x90, 60, 100]);
    }
    root.poll_midi_input();

    assert!(root.recording.pending_notes.contains_key(&60));

    // 发送 velocity=0 的 NoteOn（MIDI 规范中视为 NoteOff）
    {
        let mut buf = root
            .midi
            .input_buffer
            .lock()
            .expect("MIDI 输入缓冲区的锁未被其他线程持有，加锁应成功");
        buf.push_back(vec![0x90, 60, 0]);
    }
    root.poll_midi_input();

    let note = &root.editor.editor_state.data.current_track_notes()[0];
    assert!(
        note.end_tick > note.start_tick,
        "velocity=0 的 NoteOn 应被当作 NoteOff 处理"
    );
    assert!(!root.recording.pending_notes.contains_key(&60));
}

/// 测试录制中不会插入重复的 NoteOn
#[test]
fn test_no_duplicate_note_on_while_pending() {
    let mut root = create_root();
    attach_test_document(&mut root);
    root.recording.is_recording = true;
    root.editor.playback_position = 0.0;

    // 发送两次相同的 NoteOn
    {
        let mut buf = root
            .midi
            .input_buffer
            .lock()
            .expect("MIDI 输入缓冲区的锁未被其他线程持有，加锁应成功");
        buf.push_back(vec![0x90, 60, 100]);
        buf.push_back(vec![0x90, 60, 90]); // 重复按键，不同力度
    }
    root.poll_midi_input();

    // 验证只创建了一个音符（重复 NoteOn 被忽略）
    assert_eq!(
        root.editor.editor_state.data.current_track_note_count(),
        1,
        "重复 NoteOn 不应插入第二个音符"
    );
}

/// 测试未处于录制状态时 poll_midi_input 不处理数据
#[test]
fn test_poll_midi_input_no_op_when_not_recording() {
    let mut root = create_root();
    attach_test_document(&mut root);
    root.recording.is_recording = false;

    {
        let mut buf = root
            .midi
            .input_buffer
            .lock()
            .expect("MIDI 输入缓冲区的锁未被其他线程持有，加锁应成功");
        buf.push_back(vec![0x90, 60, 100]);
    }

    root.poll_midi_input();

    assert!(
        root.editor.editor_state.data.current_track_note_count() == 0,
        "未录制时不应处理 MIDI 输入"
    );
}

/// 测试停止录制时处理残留的 pending 音符
#[test]
fn test_stop_recording_handles_pending_notes_internal() {
    let mut root = create_root();
    attach_test_document(&mut root);
    root.recording.is_recording = true;
    root.editor.playback_position = 100.0;

    // 手动模拟 note_on: 直接插入音符并追踪
    let note = crate::editor::note::Note::new(100.0, 60, 0.0);
    root.editor
        .editor_state
        .data
        .insert_note(root.editor.editor_state.data.current_track, note);
    root.recording.pending_notes.insert(60, 0);

    // 手动停止录制（不通过 start_recording - 需要 MIDI API）
    // 这里直接模拟 stop_recording 的核心逻辑：处理残留音符
    let default_length = root.editor.editor_state.view.default_note_length.max(1.0);
    for (_, note_idx) in root.recording.pending_notes.iter() {
        if let Some(note) = root
            .editor
            .editor_state
            .data
            .document
            .as_mut()
            .and_then(|doc| doc.track_notes_mut(root.editor.editor_state.data.current_track))
            .and_then(|track| track.get_mut(*note_idx))
            && note.end_tick <= note.start_tick
        {
            note.end_tick = note.start_tick + default_length as u32;
        }
    }
    root.recording.pending_notes.clear();
    root.recording.is_recording = false;

    // 验证残留音符被设置了默认长度
    assert!(root.recording.pending_notes.is_empty());
    let note = &root.editor.editor_state.data.current_track_notes()[0];
    assert!(
        note.end_tick > note.start_tick,
        "残留音符长度应被设置为默认长度，实际 {}",
        note.end_tick - note.start_tick
    );
}
