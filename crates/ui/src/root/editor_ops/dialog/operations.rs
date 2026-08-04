//! 音符变速、批量编辑和自定义精度操作

use crate::root::Root;
use crate::toolbar;

impl Root {
    /// 应用音符变速
    pub fn apply_speed_change(&mut self, factor: f32) {
        tracing::info!("应用音符变速: 倍率={}", factor);
        self.toolbar.speed_factor = factor;
        let modified = self.editor.apply_speed_change(factor);
        if modified > 0 {
            tracing::info!("变速完成，修改了 {} 个音符", modified);
            self.update_playback_notes();
            self.editor.clear_notes_changed();
        }
    }

    /// 应用批量编辑
    pub fn apply_batch_edit(&mut self, velocity: &str, gate: &str, key: &str, tick: &str) {
        tracing::info!(
            "应用批量编辑: velocity={}, gate={}, key={}, tick={}",
            velocity,
            gate,
            key,
            tick
        );
        let max_key = if self.settings.enable_256key {
            255
        } else {
            127
        };
        let modified = self
            .editor
            .apply_batch_edit(velocity, gate, key, tick, max_key);
        if modified > 0 {
            tracing::info!("批量编辑完成，修改了 {} 个音符", modified);
            self.update_playback_notes();
            self.editor.clear_notes_changed();
        }
    }

    /// 设置自定义精度值
    pub fn set_custom_precision(&mut self, ticks: f32) {
        self.editor.set_snap_precision(ticks);
        self.editor.set_default_note_length(ticks);
        // 同步 UI 显示状态到 toolbar（status.rs 显示源），
        // 确保自定义精度生效后下拉框显示"自定义"而非旧值
        self.toolbar.note_precision = toolbar::NotePrecision::Custom;
        tracing::info!("自定义精度已设置为 {} ticks", ticks);
    }
}
