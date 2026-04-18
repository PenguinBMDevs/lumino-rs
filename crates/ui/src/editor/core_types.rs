use iced_core::Point;

#[derive(Debug, Clone, Default, PartialEq)]
pub enum EditState {
    #[default]
    Idle,
    /// 框选状态
    Selecting {
        start_pos: Point,
        current_pos: Point,
    },
    Drawing {
        start_tick: f32,
        key: u16,
        current_tick: f32,
    },
    /// 预备拖动状态：点击音符后等待判断是点击还是拖动
    PendingDrag {
        note_index: usize,
        start_pos: Point,
        original_tick: f32,
        original_key: u16,
    },
    Dragging {
        note_index: usize,
        offset_tick: f32,
        offset_key: i32,
        last_played_key: u16,
        original_tick: f32,
        original_key: u16,
    },
    ResizingStart {
        note_index: usize,
        original_tick: f32,
        original_length: f32,
    },
    ResizingEnd {
        note_index: usize,
    },
    /// 擦洗状态：在时间轴上拖动来快速定位播放位置
    Scrubbing,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HitType {
    Start,
    Middle,
    End,
}
