//! 洋葱皮 Editor API
//!
//! Editor 对洋葱皮的公开接口：缓存失效、配置读写、开关控制。
//! 查询逻辑在 `onion_skin_ops` 中。

use super::onion_skin_cache::ONION_SKIN_CACHE;
use crate::editor::{CacheInvalidation, Editor};

impl Editor {
    /// 使洋葱皮缓存全量失效（数据变化/音轨集合变化时调用）
    pub fn invalidate_onion_skin_cache(&mut self) {
        if let Ok(mut cache) = ONION_SKIN_CACHE.write()
            && cache.is_some()
        {
            tracing::debug!("Editor: 洋葱皮缓存全量清除");
            *cache = None;
        }
    }

    /// 仅标记颜色/透明度变化（无需重查 document，只重建 output）
    ///
    /// ROI：颜色变化从 O(N×T) 全量重建降为 O(C) 遍历 cells 重打包。
    /// 典型场景：用户拖拽颜色选择器、调整透明度滑块。
    pub fn invalidate_onion_skin_colors(&mut self) {
        if let Ok(mut cache) = ONION_SKIN_CACHE.write()
            && let Some(ref mut cache) = *cache
        {
            tracing::debug!("Editor: 洋葱皮颜色标记为脏");
            cache.colors_dirty = true;
        }
    }

    /// 使指定音轨的缓存失效（仅清除该轨贡献的 cells）
    ///
    /// ROI：单轨修改从 O(N×T) 全量重建降为 O(dirty_tracks)×(O(query)+O(merge))，
    /// 下次查询时只重查该轨 + 重合并，其他轨的 cells 保持不变。
    /// 典型场景：单轨音符增删、协作事件影响单轨。
    pub fn invalidate_onion_skin_cache_track(&mut self, track_idx: usize) {
        if let Ok(mut cache) = ONION_SKIN_CACHE.write()
            && let Some(ref mut cache) = *cache
        {
            cache.dirty_tracks.insert(track_idx as u16);
            tracing::debug!("Editor: 音轨 {} 标记为脏（洋葱皮）", track_idx);
        }
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
