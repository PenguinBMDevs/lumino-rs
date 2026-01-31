pub mod state;
pub mod grid;
pub mod note;

use crate::{Renderer};
use crate::Element;
use iced_widget::canvas::{self, Canvas};
use iced_core::Length;

pub use state::ViewState;
pub use grid::PianoRollGrid;

/// 钢琴卷帘编辑器
pub struct Editor {
    state: ViewState,
    // 重绘逻辑：在卷帘状态更新后重绘
    grid_cache: canvas::Cache<Renderer>, // 缓存网格绘制结果
    note_cache: canvas::Cache<Renderer>, // 缓存音符绘制结果（需要频繁更新）
}

impl Editor {
    pub fn new() -> Self {
        Self {
            state: ViewState::default(),      // 使用默认值坐标、缩放
            grid_cache: canvas::Cache::new(), // 初始化网格缓存
            note_cache: canvas::Cache::new(), // 初始化音符缓存
        }
    }

    /// 绘制钢琴卷帘网格
    pub fn view(&self) -> Element<'_> {
        // container(space()).width(Length::Fill).into() ----->原来写的
        Canvas::new(self.grid())
            .width(Length::Fill)
            .height(Length::Fill)
            .into() // 这里大小就是Canvas控件大小
    }

    fn grid(&self) -> PianoRollGrid<'_> {
        PianoRollGrid {
            state: &self.state,
            grid_cache: &self.grid_cache,
            note_cache: &self.note_cache,
        }
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}
