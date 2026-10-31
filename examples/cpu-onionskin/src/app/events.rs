//! 输入事件、编辑操作与事件循环等待策略。

use std::time::Instant;

use winit::{
    event::{ElementState, MouseButton, MouseScrollDelta},
    event_loop::ActiveEventLoop,
    keyboard::{Key, NamedKey},
};

use super::App;
use crate::{
    edit::{Drag, Editor, Tool},
    render::density,
};

impl App {
    pub(super) fn handle_mouse(&mut self, state: ElementState, button: MouseButton) {
        if self.bench_seconds.is_some() {
            return;
        }
        let Some((tick, key)) = self.canvas_tick_key() else {
            return;
        };
        let tick = self.snap_tick(tick);
        let key = key as u8;
        let ctrl = self.modifiers.control_key();
        let shift = self.modifiers.shift_key();
        match (state, button) {
            (ElementState::Pressed, MouseButton::Left) => {
                if self.editor.tool == Tool::Insert {
                    self.editor.insert_at(
                        &mut self.document,
                        &mut self.index,
                        &mut self.density,
                        self.current_track,
                        tick,
                        key,
                    );
                    self.after_edit();
                    return;
                }
                let hit =
                    Editor::hit_test(&self.document, &self.index, self.current_track, tick, key);
                if let Some((position, _)) = hit {
                    let already = self.editor.selection.contains(&position);
                    if ctrl || shift || !already {
                        self.editor.click_select(position, ctrl, shift);
                    }
                    self.editor
                        .begin_move(&self.document, self.current_track, tick, key);
                    self.drag_base = self.renderer.frame().to_vec();
                    self.needs_redraw = true;
                    self.next_frame = Instant::now();
                } else {
                    self.editor.drag = Some(Drag::Box {
                        start_tick: tick,
                        start_key: key,
                        current_tick: tick,
                        current_key: key,
                    });
                    self.drag_base = self.renderer.frame().to_vec();
                    self.needs_redraw = true;
                    self.next_frame = Instant::now();
                }
            }
            (ElementState::Released, MouseButton::Left) => {
                if self.editor.drag.is_none() {
                    return;
                }
                let changed = match self.editor.drag.as_ref() {
                    Some(Drag::Box { .. }) => {
                        self.editor.commit_box(
                            &self.document,
                            &self.index,
                            self.current_track,
                            ctrl || shift,
                        );
                        false
                    }
                    Some(Drag::Move { .. }) => self.editor.commit_move(
                        &mut self.document,
                        &mut self.index,
                        &mut self.density,
                        self.current_track,
                    ),
                    None => false,
                };
                self.drag_base.clear();
                if changed {
                    self.after_edit();
                } else {
                    self.content_dirty = true;
                    self.needs_redraw = true;
                    self.next_frame = Instant::now();
                }
            }
            (ElementState::Pressed, MouseButton::Right) => {
                let hit =
                    Editor::hit_test(&self.document, &self.index, self.current_track, tick, key);
                if let Some((position, _)) = hit {
                    self.editor.click_select(position, false, false);
                    if self.editor.delete_selected(
                        &mut self.document,
                        &mut self.index,
                        &mut self.density,
                        self.current_track,
                    ) {
                        self.after_edit();
                    }
                }
            }
            _ => {}
        }
    }

    pub(super) fn update_drag_from_mouse(&mut self) {
        let Some((tick, key)) = self.canvas_tick_key() else {
            return;
        };
        self.editor.update_drag(self.snap_tick(tick), key as u8);
        self.needs_redraw = true;
        self.next_frame = Instant::now();
    }

    fn canvas_tick_key(&self) -> Option<(u32, u16)> {
        let camera = self.camera.as_ref()?;
        let x = self.mouse.0 as f32;
        let y = self.mouse.1 as f32;
        if x < camera.keyboard_width || y < camera.ruler_height {
            return None;
        }
        let tick = camera.x_to_tick(x).max(0.0).round() as u32;
        Some((tick, camera.y_to_key(y)))
    }

    fn snap_tick(&self, tick: u32) -> u32 {
        let snap = self.editor.snap_ticks().max(1);
        (tick / snap) * snap
    }

    fn after_edit(&mut self) {
        self.content_dirty = true;
        self.needs_redraw = true;
        self.next_frame = Instant::now();
        self.idle_reported = false;
        if self.editor.density_dirty {
            self.density = density::DensityPyramid::build(&self.document, self.workers);
            self.editor.density_dirty = false;
        }
    }

    pub(super) fn handle_wheel(&mut self, delta: MouseScrollDelta) {
        let Some(camera) = self.camera.as_mut() else {
            return;
        };
        let ctrl = self.modifiers.control_key();
        let shift = self.modifiers.shift_key();
        let (dx, dy) = match delta {
            MouseScrollDelta::LineDelta(x, y) => (f64::from(x) * 120.0, f64::from(y) * 60.0),
            MouseScrollDelta::PixelDelta(position) => (position.x, position.y),
        };
        let mut dirty = false;
        if ctrl {
            let factor = 1.15_f32.powf((dy / 60.0) as f32);
            if shift {
                camera.zoom_y_at(self.mouse.1 as f32, camera.zoom_y * factor);
            } else {
                camera.zoom_x_at(self.mouse.0 as f32, camera.zoom_x * factor);
            }
            dirty = true;
        } else if shift {
            camera.scroll_x_by(-(dy + dx) as f32);
        } else {
            if dy.abs() > 0.0 {
                camera.scroll_y_by(-dy as f32);
                dirty = true;
            }
            if dx.abs() > 0.0 {
                camera.scroll_x_by(-dx as f32);
            }
        }
        if dirty {
            self.content_dirty = true;
        }
        self.needs_redraw = true;
        self.idle_reported = false;
        self.next_frame = Instant::now();
    }

    pub(super) fn handle_key(&mut self, event_loop: &ActiveEventLoop, key: &Key) {
        match key {
            Key::Named(NamedKey::Escape) => {
                if self.editor.drag.is_some() {
                    self.editor.drag = None;
                    self.drag_base.clear();
                    self.content_dirty = true;
                } else if !self.editor.selection.is_empty() {
                    self.editor.clear_selection();
                    self.content_dirty = true;
                } else {
                    event_loop.exit();
                }
            }
            Key::Named(NamedKey::Tab) => {
                self.editor.tool = match self.editor.tool {
                    Tool::Select => Tool::Insert,
                    Tool::Insert => Tool::Select,
                };
                self.editor.status = format!("工具：{}", self.editor.tool_name());
                self.content_dirty = true;
            }
            Key::Named(NamedKey::Delete) | Key::Named(NamedKey::Backspace) => {
                if self.editor.delete_selected(
                    &mut self.document,
                    &mut self.index,
                    &mut self.density,
                    self.current_track,
                ) {
                    self.after_edit();
                }
            }
            Key::Named(NamedKey::Home) => {
                if let Some(camera) = &mut self.camera {
                    camera.target_scroll_x = 0.0;
                }
                self.needs_redraw = true;
            }
            Key::Named(NamedKey::End) => {
                if let Some(camera) = &mut self.camera {
                    camera.target_scroll_x = camera.max_scroll_x();
                }
                self.needs_redraw = true;
            }
            Key::Character(text) => {
                let text = text.as_str();
                let ctrl = self.modifiers.control_key();
                let shift = self.modifiers.shift_key();
                if ctrl && (text == "z" || text == "Z") {
                    if shift {
                        if self
                            .editor
                            .redo(&mut self.document, &mut self.index, &mut self.density)
                        {
                            self.after_edit();
                        }
                    } else if self.editor.undo(
                        &mut self.document,
                        &mut self.index,
                        &mut self.density,
                    ) {
                        self.after_edit();
                    }
                    return;
                }
                if ctrl && (text == "y" || text == "Y") {
                    if self
                        .editor
                        .redo(&mut self.document, &mut self.index, &mut self.density)
                    {
                        self.after_edit();
                    }
                    return;
                }
                if ctrl && (text == "c" || text == "C") {
                    self.editor
                        .copy_selected(&self.document, self.current_track);
                    return;
                }
                if ctrl && (text == "v" || text == "V") {
                    if let Some((tick, key)) = self.canvas_tick_key() {
                        let tick = self.snap_tick(tick);
                        let key = key as u8;
                        self.editor.paste_at(
                            &mut self.document,
                            &mut self.index,
                            &mut self.density,
                            self.current_track,
                            tick,
                            key,
                        );
                        self.after_edit();
                    }
                    return;
                }
                if ctrl && (text == "a" || text == "A") {
                    self.editor.select_all(&self.document, self.current_track);
                    self.content_dirty = true;
                    return;
                }
                if text == "i" || text == "I" {
                    self.editor.tool = match self.editor.tool {
                        Tool::Select => Tool::Insert,
                        Tool::Insert => Tool::Select,
                    };
                    self.editor.status = format!("工具：{}", self.editor.tool_name());
                    self.content_dirty = true;
                } else if text == "+" || text == "=" {
                    if let Some(camera) = &mut self.camera {
                        let x = camera.width as f32 * 0.5;
                        camera.zoom_x_at(x, camera.zoom_x * 1.25);
                    }
                    self.needs_redraw = true;
                    self.content_dirty = true;
                } else if text == "-" || text == "_" {
                    if let Some(camera) = &mut self.camera {
                        let x = camera.width as f32 * 0.5;
                        camera.zoom_x_at(x, camera.zoom_x * 0.8);
                    }
                    self.needs_redraw = true;
                    self.content_dirty = true;
                } else if text == "f" || text == "F" {
                    if let Some(camera) = &mut self.camera {
                        let fit_x = camera.canvas_width() / camera.total_ticks.max(1) as f32;
                        camera.zoom_x = fit_x.clamp(0.0002, 40.0);
                        camera.zoom_y =
                            (camera.canvas_height() / camera.key_count.max(1) as f32).max(1.0);
                        camera.scroll_x = 0.0;
                        camera.target_scroll_x = 0.0;
                        camera.scroll_y = 0.0;
                    }
                    self.needs_redraw = true;
                    self.content_dirty = true;
                } else if text == "r" || text == "R" {
                    if let Some(camera) = &mut self.camera {
                        camera.zoom_x = self.initial_zoom;
                        camera.scroll_x = 0.0;
                        camera.target_scroll_x = 0.0;
                        camera.center_on_key(60);
                    }
                    self.needs_redraw = true;
                    self.content_dirty = true;
                }
            }
            _ => {}
        }
        self.idle_reported = false;
        self.next_frame = Instant::now();
    }
}
