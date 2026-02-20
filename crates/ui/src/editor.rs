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
    pub max_scroll: f32,
}

impl Editor {
    pub fn new() -> Self {
        Self {
            state: ViewState::default(),
            grid_cache: canvas::Cache::new(),
            note_cache: canvas::Cache::new(),
            max_scroll: 1000.0,
        }
    }

    // 绘制钢琴卷帘
    pub fn view(&self, on_scroll: impl Fn(f32) -> Message + 'static) -> Element<'_> {
        let grid = Canvas::new(self.grid())
            .width(Length::Fill)
            .height(Length::Fill);

        let scrollbar = scrollbar_widget::ScrollbarWidget::new(
            self.state.scroll_x,
            self.max_scroll,
            1000.0, // 宽度
            on_scroll,
        );

        column![grid, scrollbar].into()
    }

    fn grid(&self) -> PianoRollGrid<'_> {
        PianoRollGrid {
            state: &self.state,
            grid_cache: &self.grid_cache,
            note_cache: &self.note_cache,
        }
    }

    // 设置最大滚动值
    pub fn set_max_scroll(&mut self, max_scroll: f32) {
        self.max_scroll = max_scroll;
    }

    // 获取当前滚动位置
    pub fn scroll_x(&self) -> f32 {
        self.state.scroll_x
    }

    // 设置滚动位置
    pub fn set_scroll_x(&mut self, scroll_x: f32) {
        self.state.scroll_x = scroll_x.max(0.0).min(self.max_scroll);
        self.grid_cache.clear();
        self.note_cache.clear();
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}
