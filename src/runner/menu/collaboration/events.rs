//! Runner 协作：远程协作事件处理

use crate::runner::RunnerInner;

impl RunnerInner {
    /// 处理远程笔记更新
    pub(crate) fn handle_remote_note_update(&mut self, user_id: String, operation: String) {
        tracing::info!("协作: 处理远程笔记更新 - 用户: {}", user_id);

        // 解析操作
        let operation: lumino_collaboration::types::NoteBatchOperation =
            match serde_json::from_str(&operation) {
                Ok(op) => op,
                Err(e) => {
                    tracing::error!("协作: 解析笔记操作失败: {}", e);
                    return;
                }
            };

        // 应用到编辑器
        self.window_state
            .window
            .ui_mut()
            .apply_remote_note_operation(&operation);
    }

    /// 处理远程工程更新（来自其他用户，如音轨变更）
    pub(crate) fn handle_remote_project_update(&mut self, user_id: String, update_json: String) {
        tracing::info!("协作: 处理远程工程更新 - 用户: {}", user_id);

        let update: lumino_collaboration::types::ProjectUpdate =
            match serde_json::from_str(&update_json) {
                Ok(u) => u,
                Err(e) => {
                    tracing::error!("协作: 解析工程更新失败: {}", e);
                    return;
                }
            };

        match update.update_type {
            lumino_collaboration::types::ProjectUpdateType::Track => {
                if let Some(action) = update.data.get("action").and_then(|v| v.as_str())
                    && action == "add"
                {
                    let track_idx = update
                        .data
                        .get("trackIndex")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize;
                    tracing::info!("协作: 远程添加音轨 - track_index={}", track_idx);

                    self.window_state
                        .window
                        .ui_mut()
                        .add_remote_track(track_idx);
                    self.window_state.window.window().request_redraw();
                }
            }
            _ => {
                tracing::debug!("协作: 未处理的工程更新类型: {:?}", update.update_type);
            }
        }
    }
}
