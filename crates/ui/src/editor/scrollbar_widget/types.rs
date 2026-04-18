#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollbarOrientation {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Edge {
    Start,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ScrollbarState {
    #[default]
    Idle,
    Hover,
    HoverEdge(Edge),
    Dragging {
        start_pos: f32,
        start_scroll: f32,
    },
    DraggingEdge {
        start_pos: f32,
        start_zoom: f32,
        start_thumb_size: f32,
        edge: Edge,
    },
}
