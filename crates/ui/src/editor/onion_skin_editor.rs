//! 洋葱皮 Editor API
//!
//! Editor 对洋葱皮的公开接口：缓存失效、配置读写、开关控制。
//! 查询逻辑在 `onion_skin_ops` 中。

use crate::editor::{CacheInvalidation, Editor};

impl Editor {
    /// 使洋葱皮缓存全量失效（数据变化/音轨集合变化时调用）
    pub fn invalidate_onion_skin_cache(&mut self) {
        self.onion_cache_valid = false;
    }

    /// 仅标记颜色/透明度变化（已由瓦片系统替代，保留接口兼容）
    pub fn invalidate_onion_skin_colors(&mut self) {
        // 旧缓存已移除，瓦片系统通过 pool 颜色 LUT 自动更新
    }

    /// 使指定音轨的缓存失效（已由瓦片系统替代，保留接口兼容）
    pub fn invalidate_onion_skin_cache_track(&mut self, track_idx: usize) {
        let _ = track_idx;
    }

    /// 使缓存的可见音轨索引失效（音轨集合/当前音轨变化时调用）
    pub fn invalidate_onion_track_cache(&mut self) {
        self.onion_cache_valid = false;
    }

    /// 获取洋葱皮配置的可变引用
    pub fn onion_skin_config_mut(&mut self) -> &mut super::OnionSkinConfig {
        &mut self.onion_skin_config
    }

    /// 获取洋葱皮配置的引用
    pub fn onion_skin_config(&self) -> &super::OnionSkinConfig {
        &self.onion_skin_config
    }

    /// 启用洋葱皮
    pub fn enable_onion_skin(&mut self) {
        self.onion_skin_config.enable();
        self.invalidate_caches(CacheInvalidation::GRID);
        tracing::debug!("Editor: 洋葱皮已启用");
    }

    /// 禁用洋葱皮
    pub fn disable_onion_skin(&mut self) {
        self.onion_skin_config.disable();
        self.invalidate_caches(CacheInvalidation::GRID);
        tracing::debug!("Editor: 洋葱皮已禁用");
    }

    /// 切换洋葱皮开关
    pub fn toggle_onion_skin(&mut self) {
        self.onion_skin_config.toggle();
        self.invalidate_caches(CacheInvalidation::GRID);
        tracing::info!(
            "Editor: 洋葱皮已切换, is_enabled={}",
            self.onion_skin_config.is_enabled()
        );
    }

    /// 检查洋葱皮是否启用
    pub fn is_onion_skin_enabled(&self) -> bool {
        self.onion_skin_config.is_enabled()
    }

    /// 设置音轨的洋葱皮颜色（走颜色快速路径）
    pub fn set_onion_skin_color(&mut self, track_idx: usize, color: iced_core::Color) {
        self.onion_skin_config.set_track_color(track_idx, color);
        self.invalidate_onion_skin_colors();
    }

    /// 获取音轨的洋葱皮颜色
    pub fn get_onion_skin_color(&self, track_idx: usize) -> iced_core::Color {
        self.onion_skin_config.get_track_color(track_idx)
    }

    /// 设置洋葱皮透明度（走颜色快速路径）
    pub fn set_onion_skin_opacity(&mut self, opacity: f32) {
        self.onion_skin_config.set_opacity(opacity);
        self.invalidate_onion_skin_colors();
    }

    /// 获取洋葱皮透明度
    pub fn onion_skin_opacity(&self) -> f32 {
        self.onion_skin_config.opacity()
    }

    /// 设置是否显示所有音轨的洋葱皮
    pub fn set_onion_skin_show_all(&mut self, show_all: bool) {
        self.onion_skin_config.set_show_all_tracks(show_all);
        self.invalidate_caches(CacheInvalidation::GRID);
    }

    /// 添加可见音轨到洋葱皮
    pub fn add_onion_skin_track(&mut self, track_idx: usize) {
        self.onion_skin_config.add_visible_track(track_idx);
        self.invalidate_caches(CacheInvalidation::GRID);
    }

    /// 从洋葱皮移除音轨
    pub fn remove_onion_skin_track(&mut self, track_idx: usize) {
        self.onion_skin_config.remove_visible_track(track_idx);
        self.invalidate_caches(CacheInvalidation::GRID);
    }
}
