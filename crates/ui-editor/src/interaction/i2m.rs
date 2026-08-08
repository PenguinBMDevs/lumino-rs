//! 图片转 MIDI 放置模式交互
//!
//! 转换完成后进入放置模式：
//! - `Selecting`：用 Y 选择工具框选生成区域（Y 全键、X 按精度 snap），
//!   框选矩形复用 `EditState::Selecting` 绘制；
//! - `Placing`：区域框常驻显示（除非按下空白处或切换工具取消），
//!   可整体 X 向移动、拉伸左右边框（变更总显示长度）。
//!
//! 交互状态独立于 `EditState`（`I2mInteraction`），仅框选矩形绘制复用
//! `EditState::Selecting`，不耦合音符选择/选择框机制。

use crate::Editor;
use iced_core::Point;
use lumino_editor_state::editor_state::hit_test;
use lumino_editor_state::{
    EditState, I2mInteraction, ImageToMidiMode, RegionRect, SelectionHitType,
};

impl Editor {
    /// 放置模式下的鼠标按下处理
    ///
    /// - Selecting：开始框选（Y 全键、X snap）
    /// - Placing：命中区域框 → 移动/拉伸；命中空白 → 取消放置并还原显示区域
    pub(super) fn handle_i2m_pressed(&mut self, pos: Point, snapped_tick: f32, key: f32) {
        let max_key = self.editor_state.view.visible_key_count.saturating_sub(1);
        match self.editor_state.image_to_midi.mode {
            ImageToMidiMode::Selecting => {
                // 素材拖出跟随中（drag_follow 存在）：按下不进入框选，
                // 避免覆盖素材放置语义（放置由松手确认 / × 取消驱动）
                if self.editor_state.image_to_midi.drag_follow.is_some() {
                    return;
                }
                // 开始框选：复用 EditState::Selecting 绘制框选矩形
                self.editor_state
                    .image_to_midi
                    .begin_selecting(snapped_tick);
                let top_y = self.editor_state.view.key_to_y(max_key);
                let bottom_y = self.editor_state.view.key_to_y(0) + self.editor_state.view.zoom_y;
                self.editor_state.interaction.edit_state = EditState::Selecting {
                    start_tick: snapped_tick,
                    start_key: max_key,
                    current_tick: snapped_tick,
                    current_key: 0,
                    start_y: top_y,
                    current_y: bottom_y,
                };
            }
            ImageToMidiMode::Placing => {
                let hit = self.hit_test_i2m_region(pos);
                let i2m = &mut self.editor_state.image_to_midi;
                match hit {
                    Some(SelectionHitType::LeftEdge) => {
                        i2m.interaction = I2mInteraction::StretchLeft;
                        i2m.drag_start_tick = snapped_tick;
                    }
                    Some(SelectionHitType::RightEdge) => {
                        i2m.interaction = I2mInteraction::StretchRight;
                        i2m.drag_start_tick = snapped_tick;
                    }
                    Some(SelectionHitType::Inside) => {
                        i2m.interaction = I2mInteraction::Dragging;
                        i2m.drag_start_tick = snapped_tick;
                        // 素材放置：记录 Y 向拖拽基准（区域框整体上下移动）
                        i2m.drag_start_key = key;
                    }
                    None => {
                        // 按下空白处：仅清除区域框（保留预览，可重新框选）
                        i2m.clear_region();
                        self.mark_ghost_dirty();
                    }
                }
                // 区域变化 → 刷新预览渲染
                self.editor_state.image_to_midi.bump_preview_generation();
            }
            ImageToMidiMode::Inactive => {}
        }
    }

    /// 放置模式移动处理
    ///
    /// - 素材拖出跟随（Selecting + drag_follow）：预览整体跟随鼠标（X 向 + Y 向）；
    /// - Dragging：整体移动——X 向平移；素材（`allow_y_drag`）同时 Y 向平移；
    /// - StretchLeft/StretchRight：仅 X 向拉伸左右边框。
    pub(super) fn handle_i2m_moved(&mut self, snapped_tick: f32, cursor_key: f32) {
        let i2m = &mut self.editor_state.image_to_midi;
        // 素材拖出跟随：更新跟随区域（X/Y 同步）
        if i2m.mode == ImageToMidiMode::Selecting && i2m.drag_follow.is_some() {
            i2m.update_drag_follow(snapped_tick, cursor_key);
            return;
        }
        let changed = match i2m.interaction {
            I2mInteraction::Dragging => {
                // 整体 X 向平移：以鼠标 snap tick 的位移为准（累积式）
                let delta = snapped_tick - i2m.drag_start_tick;
                let changed = delta != 0.0;
                if let Some(region) = i2m.region.as_mut() {
                    region.offset_x(delta);
                    // 素材放置：整体 Y 向平移（累积式，与 X 同理）
                    if i2m.allow_y_drag {
                        let delta_key = (cursor_key - i2m.drag_start_key).round() as i32;
                        if delta_key != 0 {
                            region.offset_keys(delta_key);
                            i2m.drag_start_key = cursor_key;
                        }
                    }
                }
                i2m.drag_start_tick = snapped_tick;
                changed
            }
            I2mInteraction::StretchLeft => {
                let mut changed = false;
                if let Some(region) = i2m.region.as_mut() {
                    let old = region.tick_start;
                    region.set_left(snapped_tick);
                    changed = region.tick_start != old;
                }
                changed
            }
            I2mInteraction::StretchRight => {
                let mut changed = false;
                if let Some(region) = i2m.region.as_mut() {
                    let old = region.tick_end;
                    region.set_right(snapped_tick);
                    changed = region.tick_end != old;
                }
                changed
            }
            I2mInteraction::Selecting | I2mInteraction::None => false,
        };
        if changed {
            self.editor_state.image_to_midi.bump_preview_generation();
        }
    }

    /// 放置模式释放处理
    ///
    /// - Selecting 结束 → 用框选范围确认生成区域（进入 Placing，显示预览）
    /// - 素材拖出跟随（Selecting + drag_follow）→ 确认放置（进入 Placing）
    /// - Dragging/Stretch 结束 → 复位交互阶段
    pub(super) fn handle_i2m_released(&mut self, edit_state: EditState) {
        let i2m = &mut self.editor_state.image_to_midi;
        // 素材拖出跟随：松手确认放置（幂等：Root 侧 MaterialDragEnded 也可能确认）
        if i2m.mode == ImageToMidiMode::Selecting && i2m.drag_follow.is_some() {
            i2m.confirm_material_follow();
            self.mark_ghost_dirty();
            return;
        }
        match i2m.interaction {
            I2mInteraction::Selecting => {
                if let EditState::Selecting {
                    start_tick,
                    current_tick,
                    start_key,
                    current_key,
                    ..
                } = edit_state
                {
                    i2m.confirm_region(RegionRect::new(
                        start_tick,
                        current_tick,
                        start_key.clamp(0, 127) as u8,
                        current_key.clamp(0, 127) as u8,
                    ));
                    // 预览音符渲染依赖 region，需要刷新
                    self.editor_state.image_to_midi.bump_preview_generation();
                    self.mark_ghost_dirty();
                }
            }
            I2mInteraction::Dragging
            | I2mInteraction::StretchLeft
            | I2mInteraction::StretchRight => {
                i2m.interaction = I2mInteraction::None;
                self.mark_ghost_dirty();
            }
            I2mInteraction::None => {}
        }
    }

    /// 区域框命中测试（屏幕坐标）
    pub fn hit_test_i2m_region(&self, pos: Point) -> Option<SelectionHitType> {
        let bounds = self.i2m_region_screen_bounds()?;
        hit_test::hit_test_selection_box(bounds, (pos.x, pos.y))
    }

    /// 区域框屏幕边界 (left, right, top, bottom)
    pub fn i2m_region_screen_bounds(&self) -> Option<(f32, f32, f32, f32)> {
        let i2m = &self.editor_state.image_to_midi;
        let region = i2m.region?;
        let view = &self.editor_state.view;
        let left = view.tick_to_x(region.tick_start);
        let right = view.tick_to_x(region.tick_end);
        let top = view.key_to_y(u16::from(region.key_hi));
        let bottom = view.key_to_y(u16::from(region.key_lo)) + view.zoom_y;
        Some((left, right, top, bottom))
    }
}
