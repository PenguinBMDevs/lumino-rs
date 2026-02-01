pub mod state;
pub mod grid;
pub mod note;
pub mod scrollbar;
pub mod scrollbar_view;

use crate::Element;
use iced_widget::canvas::{self, Canvas};
use iced_widget::column;
use iced_core::Length;
use std::cell::RefCell;

pub use state::ViewState;
pub use grid::PianoRollGrid;
pub use scrollbar_view::ScrollbarView;

// 钢琴卷帘编辑器
pub struct Editor {
    pub state: ViewState,
    pub grid_cache: canvas::Cache<crate::Renderer>,
    pub note_cache: canvas::Cache<crate::Renderer>,
    pub scrollbar: RefCell<scrollbar::Scrollbar>,
    pub max_scroll: f32,
}

impl Editor {
    pub fn new() -> Self {
        Self {
            state: ViewState::default(),
            grid_cache: canvas::Cache::new(),
            note_cache: canvas::Cache::new(),
            scrollbar: RefCell::new(scrollbar::Scrollbar::new(100.0)),
            max_scroll: 1000.0,
        }
    }

    // 绘制钢琴卷帘
    pub fn view(&self) -> Element<'_> {
        let grid = Canvas::new(self.grid())
            .width(Length::Fill)
            .height(Length::Fill);

        let scrollbar = Canvas::new(ScrollbarView {
            scrollbar: &self.scrollbar,
            max_scroll: self.max_scroll,
        })
        .width(Length::Fill)
        .height(Length::Fixed(20.0));

        column![grid, scrollbar].into()
    }

    fn grid(&self) -> PianoRollGrid<'_> {
        PianoRollGrid {
            state: &self.state,
            grid_cache: &self.grid_cache,
            note_cache: &self.note_cache,
        }
    }

    // 更新视图状态
    pub fn update(&mut self) {
        // TODO: 实现滚动条与钢琴卷帘的联动滚动（我是真的不想写这个啊C）
    }

    // 设置最大滚动值
    pub fn set_max_scroll(&mut self, max_scroll: f32) {
        self.max_scroll = max_scroll;
        self.scrollbar.borrow_mut().update_thumb_from_scroll(self.state.scroll_x, max_scroll);
    }

    // 获取当前滚动位置
    pub fn scroll_x(&self) -> f32 {
        self.state.scroll_x
    }

    // 设置滚动位置
    pub fn set_scroll_x(&mut self, scroll_x: f32) {
        self.state.scroll_x = scroll_x.max(0.0).min(self.max_scroll);
        self.scrollbar.borrow_mut().update_thumb_from_scroll(self.state.scroll_x, self.max_scroll);
        self.grid_cache.clear();
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}
