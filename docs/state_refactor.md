# Editor 重构设计文档

## 当前问题

Editor struct 包含 20+ 字段，职责混乱：
- 视图状态 (state, scroll, zoom)
- 数据状态 (notes, track_notes, document)
- 交互状态 (edit_state, hover_state, selected_notes)
- 渲染相关 (grid_cache, keyboard_cache, ruler_cache)
- 历史记录 (history)
- 空间索引 (note_index)
- 协作功能 (remote_cursors)

## 目标架构

```
Editor
├── state: EditorState          // 视图 + 交互状态
├── data: EditorData            // 音符数据
├── renderer: EditorRenderer    // 渲染缓存
├── history: EditorHistory      // 历史记录
└── index: EditorIndex          // 空间索引
```

## 详细拆分

### EditorState - 视图和交互状态
```rust
pub struct EditorState {
    pub view: ViewState,              // 滚动、缩放
    pub canvas: CanvasState,          // Canvas尺寸和偏移
    pub interaction: InteractionState, // 编辑状态、悬停、选中
    pub tool: Tool,                   // 当前工具
    pub auto_scroll: AutoScrollConfig,
}
```

### EditorData - 数据管理
```rust
pub struct EditorData {
    pub current_track: usize,
    pub notes: im::Vector<Note>,
    pub track_notes: HashMap<usize, im::Vector<Note>>,
    pub document: Option<Arc<MidiDocument>>,
    pub changed: bool,
}
```

### EditorRenderer - 渲染相关
```rust
pub struct EditorRenderer {
    pub grid_cache: canvas::Cache<Renderer>,
    pub keyboard_cache: canvas::Cache<Renderer>,
    pub ruler_cache: canvas::Cache<Renderer>,
}
```

### EditorHistory - 历史记录
```rust
pub struct EditorHistory {
    inner: history::History,
}
```

### EditorIndex - 空间索引
```rust
pub struct EditorIndex {
    current: RefCell<Option<NoteSpatialIndex>>,
    tracks: RefCell<HashMap<usize, NoteSpatialIndex>>,
    dirty: Cell<bool>,
    query_cache: RefCell<Vec<usize>>,
}
```

## 迁移步骤

1. 创建新的子模块文件
2. 逐步迁移字段和方法
3. 更新所有调用点
4. 运行测试验证

## 风险评估

- 改动范围大，影响整个UI crate
- 需要更新所有 Editor 方法调用
- 建议分阶段进行，每次只迁移一个子模块
