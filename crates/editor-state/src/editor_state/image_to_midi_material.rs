//! 素材（.lmmaterial）拖出放置状态机
//!
//! 素材放置复用 `ImageToMidiState` 的预览/区域机制：
//! - `begin_material_follow`：素材拖出 → 预览跟随鼠标（X 向位置 + Y 向音高）；
//! - `update_drag_follow`：鼠标移动时更新跟随区域（无累积误差）；
//! - `confirm_material_follow`：松手确认放置（进入 Placing，√/× 出现）；
//! - `cancel_material_follow`：取消拖出（预览残留兜底清理）。

use super::image_to_midi::{
    I2mInteraction, ImageToMidiMode, ImageToMidiPreview, ImageToMidiState, RegionRect,
};

impl ImageToMidiState {
    /// 进入素材拖出跟随模式（素材拖出时由右侧栏调用）
    ///
    /// 与 `set_preview` 的区别：允许 Y 向整体移动，且预览立即以
    /// `start_tick` 为起点显示（跟随鼠标位置由 `update_drag_follow` 更新）。
    ///
    /// Y 向锚点固定为 C4（key 60）：鼠标位于 C4 时素材保持原始音高，
    /// 鼠标向高音区移动素材整体上移，向低音区移动整体下移。
    pub fn begin_material_follow(&mut self, preview: ImageToMidiPreview, start_tick: f32) {
        self.preview = Some(preview);
        self.converting = false;
        self.mode = ImageToMidiMode::Selecting;
        self.interaction = I2mInteraction::Dragging;
        self.drag_start_tick = start_tick;
        self.drag_start_key = 60.0; // C4 锚点
        self.allow_y_drag = true;
        // 初始跟随区域：从起点 tick 开始，全长 + 全 key 范围
        let width = self
            .preview
            .as_ref()
            .map(|p| p.orig_width.max(1.0))
            .unwrap_or(1.0);
        self.drag_follow = Some(RegionRect::new(start_tick, start_tick + width, 0, 127));
        self.region = None;
        self.preview_generation = 0;
        self.bump_preview_generation();
    }

    /// 更新素材拖出跟随位置（鼠标位置变化时调用）
    ///
    /// 整体平移跟随区域：以拖出起点为基准计算 tick/key 位移。
    pub fn update_drag_follow(&mut self, cursor_tick: f32, cursor_key: f32) {
        let Some(follow) = self.drag_follow.as_mut() else {
            return;
        };
        let base_tick = self.drag_start_tick;
        let base_key = self.drag_start_key;
        let delta_tick = cursor_tick - base_tick;
        let delta_key = (cursor_key - base_key).round();
        // 平移基于"原始锚点"计算：直接重建区域保证无累积误差
        let width = follow.width();
        let mut next = RegionRect::new(
            base_tick + delta_tick,
            base_tick + delta_tick + width,
            0,
            127,
        );
        next.offset_keys(delta_key as i32);
        *follow = next;
        self.bump_preview_generation();
    }

    /// 素材拖出结束：以跟随区域确认放置（进入 Placing）
    pub fn confirm_material_follow(&mut self) {
        if let Some(follow) = self.drag_follow.take() {
            self.region = Some(follow);
            self.mode = ImageToMidiMode::Placing;
            self.interaction = I2mInteraction::None;
            self.allow_y_drag = true;
            self.bump_preview_generation();
        }
    }

    /// 素材拖出取消（拖出到无效区域 / 主动取消）
    pub fn cancel_material_follow(&mut self) {
        self.drag_follow = None;
        if self.region.is_none() {
            *self = Self::default();
        } else {
            self.interaction = I2mInteraction::None;
            self.bump_preview_generation();
        }
    }

    /// 预览音符在区域内的显示 key
    ///
    /// - I2M 放置（`allow_y_drag = false`）：key 保持原始值，区域 key 范围仅作显示窗口；
    /// - 素材放置（`allow_y_drag = true`）：key 随区域 Y 向整体偏移（跟随鼠标上下移动）。
    pub fn note_screen_key(&self, orig_key: u8) -> u8 {
        if !self.allow_y_drag {
            return orig_key;
        }
        let Some(region) = self.active_region() else {
            return orig_key;
        };
        let offset = (region.key_hi as i32) - 127;
        (orig_key as i32 + offset).clamp(0, 127) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor_state::image_to_midi::PreviewNote;

    #[test]
    fn test_material_follow_flow() {
        let mut state = ImageToMidiState::default();
        state.begin_material_follow(
            ImageToMidiPreview {
                tracks: vec![vec![PreviewNote {
                    tick: 0.0,
                    length: 100.0,
                    key: 60,
                }]],
                orig_width: 100.0,
            },
            100.0,
        );
        assert_eq!(state.mode, ImageToMidiMode::Selecting);
        assert!(state.allow_y_drag);
        assert!(state.drag_follow.is_some());
        // 初始：鼠标在 C4 → 音符保持原始 key
        assert_eq!(state.note_screen_key(60), 60);

        // X 向移动
        state.update_drag_follow(300.0, 60.0);
        let follow = state.drag_follow.expect("应有跟随区域");
        assert_eq!(follow.tick_start, 300.0);
        assert_eq!(follow.tick_end, 400.0);
        // Y 向移动（鼠标上移到 key 72 → 素材整体上移 12 个半音）
        state.update_drag_follow(300.0, 72.0);
        assert_eq!(state.note_screen_key(60), 72);
        assert_eq!(state.note_screen_key(48), 60);

        // 确认放置 → 进入 Placing，drag_follow 移交为 region
        state.confirm_material_follow();
        assert_eq!(state.mode, ImageToMidiMode::Placing);
        assert!(state.region.is_some());
        assert!(state.drag_follow.is_none());
        // 放置后仍允许 Y 向移动（素材语义）
        assert!(state.allow_y_drag);
    }

    #[test]
    fn test_material_follow_cancel() {
        let mut state = ImageToMidiState::default();
        state.begin_material_follow(
            ImageToMidiPreview {
                tracks: Vec::new(),
                orig_width: 50.0,
            },
            0.0,
        );
        assert!(state.is_active());
        state.cancel_material_follow();
        assert_eq!(state.mode, ImageToMidiMode::Inactive);
    }

    #[test]
    fn test_i2m_note_key_unchanged_with_region() {
        // I2M 语义：区域 key 范围只作显示窗口，音符 key 不随区域变化
        let mut state = ImageToMidiState::default();
        state.preview = Some(ImageToMidiPreview {
            tracks: vec![vec![PreviewNote {
                tick: 0.0,
                length: 10.0,
                key: 60,
            }]],
            orig_width: 100.0,
        });
        state.confirm_region(RegionRect::new(0.0, 100.0, 40, 80));
        assert_eq!(state.note_screen_key(60), 60);
    }

    #[test]
    fn test_material_stretch_updates_all_track_note_lengths() {
        // 复现验证：多轨素材放置后拉伸区域框，所有轨道的音符长度必须等比变化
        let mut state = ImageToMidiState::default();
        let preview = ImageToMidiPreview {
            tracks: vec![
                vec![PreviewNote {
                    tick: 0.0,
                    length: 100.0,
                    key: 60,
                }],
                vec![PreviewNote {
                    tick: 50.0,
                    length: 200.0,
                    key: 72,
                }],
            ],
            orig_width: 300.0,
        };
        state.begin_material_follow(preview, 0.0);
        // 拖到 tick 1000 处并确认放置
        state.update_drag_follow(1000.0, 60.0);
        state.confirm_material_follow();
        assert_eq!(state.mode, ImageToMidiMode::Placing);
        assert_eq!(state.region.expect("应有区域").width(), 300.0);

        // 拉伸前：scale_x = 1
        let before: Vec<(f32, u8, f32)> = state.track_screen_notes(0);
        let before1: Vec<(f32, u8, f32)> = state.track_screen_notes(1);
        assert_eq!(before[0].2, 100.0);
        assert_eq!(before1[0].2, 200.0);

        // 拉伸右边界到 1600 → width = 600 → scale_x = 2
        state.region.as_mut().expect("区域存在").set_right(1600.0);
        state.bump_preview_generation();

        let after: Vec<(f32, u8, f32)> = state.track_screen_notes(0);
        let after1: Vec<(f32, u8, f32)> = state.track_screen_notes(1);
        assert_eq!(after[0].2, 200.0, "轨 0 音符长度应变等比变化");
        assert_eq!(after1[0].2, 400.0, "轨 1 音符长度应变等比变化");
        assert_eq!(after[0].0, 1000.0, "轨 0 音符起点 tick 保持区域左边界");
        assert_eq!(after1[0].0, 1100.0, "轨 1 音符起点 tick 等比映射");
    }
}
