//! 素材库交互处理器（右侧栏素材面板）
//!
//! 素材（.lmmaterial）的完整生命周期：
//! - 列表扫描（内置 + 用户配置目录）；
//! - 本地导入（复制到用户素材目录）；
//! - 拖出放置（加载到内存 → 预览跟随鼠标 → √/× 确认写入）；
//! - 右键菜单（重命名 / 删除 / 上传到云）。
//!
//! 2026-08-18 拆分（原文件 803 行 → 按职责分模块）：
//! - `scan`：素材列表后台扫描与本地导入
//! - `placement`：放置确认写入（逐轨写入 / 自动建轨 / CreateOp 历史）
//! - `menu`：右键菜单 / 重命名 / 删除确认
//! - `cloud`：素材上传到云
//! - `tests`：单元测试
//!
//! 本文件保留素材拖出跟随（按下即生效的放置预览）。

mod cloud;
mod menu;
mod placement;
mod scan;
#[cfg(test)]
mod tests;

use crate::root::Root;

impl Root {
    /// 素材项按下：立即进入拖出跟随模式（预览跟随鼠标）
    ///
    /// 素材预览在扫描时已预解析缓存（`MaterialEntry.preview`），
    /// 此处**同步**启动——按下即生效，不依赖异步轮询（修复拖放失效：
    /// 此前异步加载 + 消息驱动 poll，素材就绪时无消息触发轮询，拖放无响应）。
    pub(super) fn start_material_drag(&mut self, index: usize) {
        let Some(entry) = self.right_sidebar.materials.entries.get(index) else {
            return;
        };
        let Some(preview) = entry.preview.clone() else {
            tracing::warn!("素材 {} 无可用的放置预览，拖出已忽略", entry.name);
            return;
        };
        self.editor
            .editor_state
            .image_to_midi
            .begin_material_follow(preview, 0.0);
        self.editor
            .invalidate_caches(lumino_ui_editor::CacheInvalidation::ALL);
        tracing::info!("素材 {} 已进入拖出跟随模式", entry.name);
    }

    /// 清理过期的素材拖出跟随（鼠标已释放且未在卷帘内确认放置）
    ///
    /// 素材拖出 = 右侧栏按下 → 移入卷帘松手放置；若在右侧栏/空白处松手
    /// （卷帘 released 不会触发），跟随预览会残留——本方法兜底取消。
    pub(crate) fn cancel_stale_material_follow(&mut self) {
        use lumino_editor_state::ImageToMidiMode;
        let i2m = &self.editor.editor_state.image_to_midi;
        if i2m.mode == ImageToMidiMode::Selecting && i2m.drag_follow.is_some() {
            self.editor
                .editor_state
                .image_to_midi
                .cancel_material_follow();
            self.editor
                .invalidate_caches(lumino_ui_editor::CacheInvalidation::ALL);
            tracing::debug!("素材拖出已取消（未在卷帘内放置）");
        }
    }
}
