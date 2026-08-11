//! 曲线工具贝塞尔路径状态测试

use super::*;

#[test]
fn test_anchor_chain_flow() {
    let mut state = LineToolState::default();
    assert!(!state.has_anchor());
    assert!(!state.is_complete());

    state.paths.push(Vec::new());
    state.push_anchor(0, (0.0, 60.0));
    assert!(state.has_anchor());
    assert_eq!(state.paths[0].len(), 1);
    assert!(!state.is_complete());

    state.push_anchor(0, (1920.0, 64.0));
    assert!(state.is_complete());
    assert_eq!(state.paths[0].len(), 2);
}

#[test]
fn test_creating_path_index() {
    let mut state = LineToolState::default();
    assert_eq!(state.creating_path(), None);
    state.paths.push(vec![BezierAnchor::new((0.0, 60.0))]);
    assert_eq!(state.creating_path(), Some(0));
    state.push_anchor(0, (1920.0, 64.0));
    assert_eq!(state.creating_path(), None, "完整路径不再创建中");
    // 新路径：未完整 → 又是创建中
    state.paths.push(vec![BezierAnchor::new((0.0, 70.0))]);
    assert_eq!(state.creating_path(), Some(1));
}

#[test]
fn test_insert_anchor_at_segment() {
    let mut state = LineToolState::default();
    state.paths.push(Vec::new());
    state.push_anchor(0, (0.0, 60.0));
    state.push_anchor(0, (1920.0, 64.0));
    assert!(state.insert_anchor_at(0, 1, (960.0, 62.0)));
    assert_eq!(state.paths[0].len(), 3);
    assert_eq!(state.paths[0][1].pos, (960.0, 62.0));
    assert!(state.paths[0][1].handles_auto);
    assert!(!state.insert_anchor_at(0, 0, (1.0, 1.0)), "索引 0 非法");
    assert!(!state.insert_anchor_at(1, 1, (1.0, 1.0)), "路径越界");
}

#[test]
fn test_delete_anchor_only_middle() {
    let mut state = LineToolState::default();
    state.paths.push(Vec::new());
    state.push_anchor(0, (0.0, 60.0));
    state.push_anchor(0, (960.0, 62.0));
    state.push_anchor(0, (1920.0, 64.0));
    assert!(!state.delete_anchor(0, 0));
    assert!(!state.delete_anchor(0, 2));
    assert!(state.delete_anchor(0, 1));
    assert_eq!(state.paths[0].len(), 2);
}

#[test]
fn test_auto_handles_after_push() {
    let mut state = LineToolState::default();
    state.paths.push(Vec::new());
    state.push_anchor(0, (0.0, 60.0));
    state.push_anchor(0, (1920.0, 64.0));
    assert_eq!(state.paths[0][0].out_handle, (640.0, 4.0 / 3.0));
    assert_eq!(state.paths[0][1].in_handle, (-640.0, -4.0 / 3.0));
}

// ── 历史（撤销/重做） ──

#[test]
fn test_undo_redo_path() {
    let mut state = LineToolState::default();
    state.paths.push(Vec::new());
    state.push_anchor(0, (0.0, 60.0));
    state.push_anchor(0, (1920.0, 64.0));
    // 记录基准状态（模拟正常交互创建后的历史栈）
    state.push_path_history();
    // 插入锚点 + 记录
    state.insert_anchor_at(0, 1, (960.0, 62.0));
    state.push_path_history();
    assert_eq!(state.paths[0].len(), 3);
    // 撤销：插入的锚点消失（恢复到基准）
    assert!(state.undo_path());
    assert_eq!(state.paths[0].len(), 2);
    // 再撤销：删除整条路径（基准之前是空）
    assert!(state.undo_path());
    assert!(state.paths.is_empty());
    assert!(!state.undo_path(), "无可撤销");
    // 重做两步：路径与锚点依次恢复
    assert!(state.redo_path());
    assert_eq!(state.paths[0].len(), 2);
    assert!(state.redo_path());
    assert_eq!(state.paths[0].len(), 3);
    assert!(!state.redo_path(), "无可重做");
}

#[test]
fn test_create_path_merged_into_one_undo() {
    let mut state = LineToolState::default();
    // 创建曲线：第一次 push 记录、第二次 push 更新栈顶合并
    state.paths.push(Vec::new());
    state.push_anchor(0, (0.0, 60.0));
    state.push_path_history();
    state.last_push_path = Some(0);
    state.push_anchor(0, (1920.0, 64.0));
    state.update_top_path_history();
    // 一次撤销删除整条曲线
    assert!(state.undo_path());
    assert!(state.paths.is_empty(), "一次撤销应删除整条曲线");
}

#[test]
fn test_push_history_truncates_redo() {
    let mut state = LineToolState::default();
    state.paths.push(Vec::new());
    state.push_anchor(0, (0.0, 60.0));
    state.push_anchor(0, (1920.0, 64.0));
    state.push_path_history(); // 基准
    // 移动锚点 + 记录
    state.paths[0][0].pos = (100.0, 60.0);
    state.push_path_history();
    state.undo_path();
    assert!(state.can_redo_path());
    // 新操作（先恢复再改）截断 redo 分支
    state.redo_path();
    state.paths[0][0].pos = (200.0, 60.0);
    state.push_path_history();
    assert!(!state.can_redo_path());
}

#[test]
fn test_reset_clears_all_and_history() {
    let mut state = LineToolState::default();
    state.paths.push(Vec::new());
    state.push_anchor(0, (0.0, 60.0));
    state.push_anchor(0, (1920.0, 64.0));
    state.push_path_history();
    state.reset();
    assert_eq!(state, LineToolState::default());
}
