use super::common::create_root;

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
    assert_eq!(root.editor.editor_state.data.notes.len(), 1);
    let note = &root.editor.editor_state.data.notes[0];
    assert_eq!(note.key, 60);
    assert_eq!(note.velocity, 100);
    assert!(note.tick >= 0.0, "音符 tick 应 >= 0，实际 {}", note.tick);

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
    assert_eq!(root.editor.editor_state.data.notes.len(), 1);
    let note = &root.editor.editor_state.data.notes[0];
    assert!(note.length > 0.0, "音符长度应大于 0，实际 {}", note.length);

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

    let note = &root.editor.editor_state.data.notes[0];
    assert!(
        note.length > 0.0,
        "velocity=0 的 NoteOn 应被当作 NoteOff 处理"
    );
    assert!(!root.recording.pending_notes.contains_key(&60));
}

/// 测试录制中不会插入重复的 NoteOn
#[test]
fn test_no_duplicate_note_on_while_pending() {
    let mut root = create_root();
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
        root.editor.editor_state.data.notes.len(),
        1,
        "重复 NoteOn 不应插入第二个音符"
    );
}

/// 测试未处于录制状态时 poll_midi_input 不处理数据
#[test]
fn test_poll_midi_input_no_op_when_not_recording() {
    let mut root = create_root();
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
        root.editor.editor_state.data.notes.is_empty(),
        "未录制时不应处理 MIDI 输入"
    );
}

/// 测试停止录制时处理残留的 pending 音符
#[test]
fn test_stop_recording_handles_pending_notes_internal() {
    let mut root = create_root();
    root.recording.is_recording = true;
    root.editor.playback_position = 100.0;

    // 手动模拟 note_on: 直接插入音符并追踪
    let note = crate::editor::note::Note::new(100.0, 60, 0.0);
    root.editor.editor_state.data.notes.push_back(note);
    root.recording.pending_notes.insert(60, 0);

    // 手动停止录制（不通过 start_recording - 需要 MIDI API）
    // 这里直接模拟 stop_recording 的核心逻辑：处理残留音符
    let default_length = root.editor.editor_state.view.default_note_length.max(1.0);
    for (_, note_idx) in root.recording.pending_notes.iter() {
        if let Some(note) = root.editor.editor_state.data.notes.get_mut(*note_idx)
            && note.length <= 0.0
        {
            note.length = default_length;
        }
    }
    root.recording.pending_notes.clear();
    root.recording.is_recording = false;

    // 验证残留音符被设置了默认长度
    assert!(root.recording.pending_notes.is_empty());
    let note = &root.editor.editor_state.data.notes[0];
    assert!(
        note.length > 0.0,
        "残留音符长度应被设置为默认长度，实际 {}",
        note.length
    );
}
