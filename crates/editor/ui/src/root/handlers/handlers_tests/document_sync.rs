//! 新建/恢复音轨后 document 同步与 PPQ 保存链路测试

use super::*;

/// 读取 document 当前音轨数（无 document 时为 0）
fn doc_track_count(root: &Root) -> usize {
    root.editor
        .editor_state
        .data
        .document
        .as_ref()
        .map(|d| d.track_count())
        .unwrap_or(0)
}

/// 核心 BUG 复现：新建音轨 → 切换到新轨 → 放置音符。
///
/// 修复前：`AddTrack` 只更新 sidebar.tracks（UI 列表），`MidiDocument.notes`
/// 未同步扩轨；新轨 `insert_note` 因 track_id 越界静默返回 false，音符被丢弃，
/// 表现为"只能在第一个音轨放置音符"。
#[test]
fn test_add_track_expands_document_for_note_placement() {
    let mut root = create_root();
    attach_test_document(&mut root); // document 2 轨，sidebar 默认 2 轨，current_track=1

    // 用户操作：新建音轨
    root.handle_sidebar_event(crate::sidebar::Event::AddTrack);

    // document 必须同步扩展为 3 轨
    assert_eq!(
        doc_track_count(&root),
        3,
        "AddTrack 后 document 应扩展为 3 轨"
    );

    // 新音轨（id=2）必须能插入音符——修复前此处静默失败
    let new_id = root
        .sidebar
        .tracks
        .last()
        .map(|t| t.id)
        .expect("AddTrack 后 sidebar 应包含新音轨");
    assert_eq!(new_id, 2, "新音轨 id 应为 2");
    let inserted = root
        .editor
        .editor_state
        .data
        .insert_note(new_id, crate::editor::note::Note::new(0.0, 60, 480.0));
    assert!(inserted, "新音轨应能插入音符");
    assert_eq!(
        root.editor.editor_state.data.track_notes(new_id).len(),
        1,
        "新音轨应包含 1 个音符"
    );
}

/// 同类路径：在指定音轨上方/下方添加音轨，document 同样需要扩轨
#[test]
fn test_track_add_above_below_expands_document() {
    let mut root = create_root();
    attach_test_document(&mut root);

    root.handle_sidebar_event(crate::sidebar::Event::TrackAddAbove(1));
    root.handle_sidebar_event(crate::sidebar::Event::TrackAddBelow(1));

    // 两次添加：sidebar 4 轨（0/1/2/3），document 必须覆盖到最大 id
    assert_eq!(
        doc_track_count(&root),
        4,
        "添加上/下方音轨后 document 应扩展为 4 轨"
    );

    let ids: Vec<usize> = root.sidebar.tracks.iter().map(|t| t.id).collect();
    for id in ids {
        let inserted = root
            .editor
            .editor_state
            .data
            .insert_note(id, crate::editor::note::Note::new(0.0, 60, 480.0));
        assert!(inserted, "音轨 id={} 应能插入音符", id);
    }
}

/// 同类路径：协作远程音轨加入后，document 必须扩轨（此前只 push sidebar.tracks）
#[test]
fn test_add_remote_track_expands_document() {
    let mut root = create_root();
    attach_test_document(&mut root);

    root.add_remote_track(5);

    assert_eq!(doc_track_count(&root), 6, "协作远程音轨应同步扩展 document");
    let inserted = root
        .editor
        .editor_state
        .data
        .insert_note(5, crate::editor::note::Note::new(0.0, 60, 480.0));
    assert!(inserted, "协作远程音轨应能插入音符");
}

/// 同类路径：恢复已删除音轨（track_id 可能大于当前 document 轨数）
#[test]
fn test_apply_track_restored_expands_document() {
    use lumino_message::events::window::track::{TrackDeletionNote, TrackDeletionPayload};

    let mut root = create_root();
    attach_test_document(&mut root);

    let payload = TrackDeletionPayload {
        track_id: 5,
        track_name: "Restored".to_string(),
        port: 0,
        channel: 0,
        is_drum: false,
        max_tick: 480,
        original_index: 1,
        notes: vec![TrackDeletionNote {
            start_tick: 0,
            end_tick: 480,
            key: 60,
            velocity: 100,
            channel: 0,
            port: 0,
        }],
    };
    root.apply_track_restored(payload);

    assert_eq!(
        doc_track_count(&root),
        6,
        "恢复音轨后 document 应扩展为 6 轨"
    );
    assert_eq!(
        root.editor.editor_state.data.track_notes(5).len(),
        1,
        "恢复的音符应写入 document"
    );
}

// ── PPQ 修改贯穿保存链路（BUG 回归：工程文件落盘旧值 480） ──────────────

/// BUG 复现：工具栏修改 PPQ 只更新视图状态，`document.division`（单一权威源）
/// 保持旧值 480；保存工程时 `from_midi_document` 读取 document.division，
/// 导致新工程 PPQ 丢失、工程文件永远落盘 480。
#[test]
fn test_set_ppq_syncs_document_division() {
    let mut root = create_root();
    attach_test_document(&mut root);

    // 初始：视图默认 1920，测试文档构造为 480（真实场景下新工程空文档
    // 与视图同源，此处故意制造不一致以验证 set_ppq 能把 document 拉齐）
    assert_eq!(root.editor.editor_state.view.ppq, 1920);
    assert_eq!(
        root.editor
            .editor_state
            .data
            .document
            .as_ref()
            .expect("测试文档应已挂载")
            .division,
        480
    );

    // 用户经工具栏把 PPQ 改为 960
    root.set_ppq(960);

    // 视图状态同步
    assert_eq!(root.editor.editor_state.view.ppq, 960);
    // 保存链路权威源必须同步——修复前此处保持 480，工程文件落盘错误
    assert_eq!(
        root.editor
            .editor_state
            .data
            .document
            .as_ref()
            .expect("测试文档应已挂载")
            .division,
        960,
        "document.division 应随 PPQ 修改同步，保证工程文件保存新 PPQ"
    );
}

/// 无 document 时（编辑器已重置、空白工程未初始化）set_ppq 不应 panic
#[test]
fn test_set_ppq_without_document_no_panic() {
    let mut root = create_root();

    root.set_ppq(960);

    assert_eq!(root.editor.editor_state.view.ppq, 960);
    assert!(root.editor.editor_state.data.document.is_none());
}
