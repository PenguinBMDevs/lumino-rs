pub mod state;
pub mod grid;
pub mod note;
pub mod scrollbar;
pub mod scrollbar_widget;

use crate::{Element, Message};
use iced_widget::canvas::{self, Canvas};
use iced_widget::column;
use iced_core::Length;

pub use state::ViewState;
pub use grid::PianoRollGrid;

// 钢琴卷帘编辑器
pub struct Editor {
    pub state: ViewState,
    pub grid_cache: canvas::Cache<crate::Renderer>,
    pub note_cache: canvas::Cache<crate::Renderer>,
    pub max_scroll_x: f32,
    pub max_scroll_y: f32,
}

impl Editor {
    pub fn new() -> Self {
        let mut editor = Self {
            state: ViewState::default(),
            grid_cache: canvas::Cache::new(),
            note_cache: canvas::Cache::new(),
            max_scroll_x: 1000.0,
            max_scroll_y: 0.0, // 稍后计算
        };
        // 根据visible_key_count计算max_scroll_y
        editor.max_scroll_y = editor.state.visible_key_count as f32 * editor.state.zoom_y;
        editor
    }

    // 绘制钢琴卷帘
    pub fn view(&self, on_scroll_x: impl Fn(f32) -> Message + 'static, on_scroll_y: impl Fn(f32) -> Message + 'static) -> Element<'_> {
        let grid = Canvas::new(self.grid())
            .width(Length::Fill)
            .height(Length::Fill);

        let horizontal_scrollbar = scrollbar_widget::ScrollbarWidget::horizontal(
            self.state.scroll_x,
            self.max_scroll_x,
            on_scroll_x,
        );

        let vertical_scrollbar = scrollbar_widget::ScrollbarWidget::vertical(
            self.state.scroll_y,
            self.max_scroll_y,
            on_scroll_y,
        );

        let content_with_vscroll = iced_widget::row![grid, vertical_scrollbar];

        iced_widget::column![content_with_vscroll, horizontal_scrollbar].into()
    }

    fn grid(&self) -> PianoRollGrid<'_> {
        PianoRollGrid {
            state: &self.state,
            grid_cache: &self.grid_cache,
            note_cache: &self.note_cache,
        }
    }

    // 设置最大滚动值
    pub fn set_max_scroll_x(&mut self, max_scroll: f32) {
        self.max_scroll_x = max_scroll;
    }

    pub fn set_max_scroll_y(&mut self, max_scroll: f32) {
        self.max_scroll_y = max_scroll;
    }

    // 获取当前滚动位置
    pub fn scroll_x(&self) -> f32 {
        self.state.scroll_x
    }

    pub fn scroll_y(&self) -> f32 {
        self.state.scroll_y
    }

    // 设置滚动位置
    pub fn set_scroll_x(&mut self, scroll_x: f32) {
        self.state.scroll_x = scroll_x.max(0.0).min(self.max_scroll_x);
        self.grid_cache.clear();
        self.note_cache.clear();
    }

    pub fn set_scroll_y(&mut self, scroll_y: f32) {
        self.state.scroll_y = scroll_y.max(0.0).min(self.max_scroll_y);
        self.grid_cache.clear();
        self.note_cache.clear();
    }

    // 设置可见琴键数量（1-256）
    pub fn set_visible_key_count(&mut self, count: u16) {
        let clamped_count = count.clamp(1, 256);
        self.state.visible_key_count = clamped_count;
        // 联动更新纵向滚动范围
        // 滚动范围应该是总高度减去视口高度，但由于这里无法直接获取视口高度，
        // 我们将 max_scroll_y 设置为总高度，在 scrollbar_widget 中处理比例。
        // 实际上，如果 max_scroll_y 是总高度，那么滚动到底部时，最下面的琴键会在视口顶部。
        // 为了让最下面的琴键在视口底部，我们需要在外部（如 view 方法中）或者在 grid 绘制时处理。
        // 目前先保持为总高度，确保能滚动到所有琴键。
        self.max_scroll_y = clamped_count as f32 * self.state.zoom_y;
        // 确保当前滚动位置不超过新范围
        if self.state.scroll_y > self.max_scroll_y {
            self.state.scroll_y = self.max_scroll_y;
        }
        self.grid_cache.clear();
        self.note_cache.clear();
    }

    // 获取可见琴键数量
    pub fn visible_key_count(&self) -> u16 {
        self.state.visible_key_count
    }

    // 设置键盘宽度
    pub fn set_keyboard_width(&mut self, width: f32) {
        let clamped_width = width.max(0.0);
        self.state.keyboard_width = clamped_width;
        self.grid_cache.clear();
        self.note_cache.clear();
    }

    // 获取键盘宽度
    pub fn keyboard_width(&self) -> f32 {
        self.state.keyboard_width
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}
