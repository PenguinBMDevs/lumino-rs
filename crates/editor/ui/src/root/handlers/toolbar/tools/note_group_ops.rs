//! 工具栏「成组类」音符操作：连奏（Tie）与分割 / 合并（Split / Glue）
//!
//! 从 `note_ops.rs` 拆出以控制单文件行数。两者共同点是**都依赖选中集合之间的
//! 相邻 / 同 key 关系**（连奏连接同 key 相邻音符，合并连接首尾相接音符），
//! 与量化 / 翻转 / 移调这类「逐音符独立变换」的关注点不同。
//!
//! # P1-4：走带模式的作用域隔离
//!
//! 二者在走带视图下均受 [`ToolbarHandler::arrangement_batch_gate`] 拦截——
//! 走带选区（`arrange_selection`，跨轨矩形）与它们作用的卷帘选区
//! （`interaction.selected_notes`，单轨索引位图）是两套东西。走带版实现属
//! P2 能力补齐；本次修复保证的是**绝不回退到卷帘选区去误伤**。

use super::ToolbarHandler;
use crate::root::Root;

impl ToolbarHandler {
    /// 处理连奏操作
    pub(crate) fn handle_toolbar_tie(&self, root: &mut Root, event: &crate::toolbar::Event) {
        if !matches!(event, crate::toolbar::Event::Tie) {
            return;
        }

        if !Self::arrangement_batch_gate(root, "连奏") {
            return;
        }

        tracing::info!("Root: 执行音符连奏操作");

        // 必须有选中音符才能连奏
        if root
            .editor
            .editor_state
            .interaction
            .selected_notes
            .is_empty()
        {
            tracing::debug!("Root: 没有选中音符，不执行连奏");
            return;
        }

        let tied = root.editor.tie_selected_notes();

        if tied > 0 {
            tracing::info!("Root: 连奏完成，连接了 {} 个音符", tied);
            root.update_playback_notes();
            root.editor.clear_notes_changed();
        } else {
            tracing::debug!("Root: 没有音符被连奏（需至少 2 个同 Key 的选中音符）");
        }
    }

    /// 处理分割/合并操作
    pub(crate) fn handle_toolbar_split_glue(&self, root: &mut Root, event: &crate::toolbar::Event) {
        // 分支前先过走带闸门：走带模式下无论分割还是合并都拒绝（合并对空选区有
        // 无害的默认值，分割则会命中「按当前轨索引」的错误作用域）
        if matches!(
            event,
            crate::toolbar::Event::Split | crate::toolbar::Event::Glue
        ) && !Self::arrangement_batch_gate(root, "分割/合并")
        {
            return;
        }
        match event {
            crate::toolbar::Event::Split => {
                // 分割选中音符：在音符中间位置分割
                let selected: Vec<usize> = root
                    .editor
                    .editor_state
                    .interaction
                    .selected_notes
                    .iter()
                    .collect();

                if selected.is_empty() {
                    tracing::debug!("Root: 分割操作 - 没有选中音符");
                    return;
                }

                let mut split_count = 0usize;
                // 从大到小处理，避免索引偏移
                let mut indices: Vec<usize> = selected;
                indices.sort_by(|a, b| b.cmp(a));
                indices.dedup();

                root.editor.push_history();

                for &idx in &indices {
                    if let Some(note) = root.editor.editor_state.data.current_track_notes().get(idx)
                    {
                        let split_tick =
                            note.start_tick as f32 + (note.end_tick - note.start_tick) as f32 / 2.0;
                        root.editor.split_note(idx, split_tick);
                        split_count += 1;
                    }
                }

                if split_count > 0 {
                    tracing::info!("Root: 分割完成 - 分割了 {} 个音符", split_count);
                    root.update_playback_notes();
                    root.editor.clear_notes_changed();
                    root.editor.editor_state.interaction.selected_notes.clear();
                }
            }
            crate::toolbar::Event::Glue => {
                let merged = root.editor.glue_selected_notes();
                if merged > 0 {
                    tracing::info!("Root: 合并完成 - 合并了 {} 组音符", merged);
                    root.update_playback_notes();
                    root.editor.clear_notes_changed();
                } else {
                    tracing::debug!("Root: 合并操作 - 没有可合并的音符");
                }
            }
            _ => {}
        }
    }
}
