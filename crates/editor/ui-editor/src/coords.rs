//! 坐标转换 — 委托到 ViewState
impl super::Editor {
    pub(super) fn tick_to_x(&self, tick: f32) -> f32 {
        self.editor_state.view.tick_to_x(tick)
    }
    pub(super) fn key_to_y(&self, key: u16) -> f32 {
        self.editor_state.view.key_to_y(key)
    }
    pub(super) fn x_to_tick(&self, x: f32) -> f32 {
        self.editor_state.view.x_to_tick(x)
    }
    pub(super) fn y_to_key(&self, y: f32) -> u16 {
        self.editor_state.view.y_to_key(y)
    }
    pub(super) fn snap_tick(&self, tick: f32) -> f32 {
        self.editor_state.view.snap_tick(tick)
    }
}
