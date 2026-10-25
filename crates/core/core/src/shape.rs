//! 形状数据模型（Spiderweb 图形工具移植底座 · S0-1）
//!
//! 本模块只承载**数据与校验**，不含几何求交、不生成音符——那是后续算子（T1）与再生成管线的事。
//!
//! ## 设计约束
//!
//! 1. **形状是文档对象**：随工程保存、参数可回改、可与它生成的音符同组撤销。
//!    因此这里的类型必须 `serde` 化且字段定宽（`u16`/`u64`，不用 `usize`），
//!    保证跨平台落盘字节一致。
//! 2. **几何与参数分离**：`geometry` 是当前几何，`ops` 是**有序修饰链**——
//!    每一步作用在上一步输出的音符上（Spiderweb `notes/fx.py` 的语义）。
//! 3. **视图比例 `k` 必须随形状存储**：圆弧、沿途突起、遮罩带的"圆度"按**绘制时**
//!    的屏幕比例定义（Spiderweb `notes/arc.py` 的 `k` = beats per key）。
//!    丢了它，同一形状换个缩放就会走形——这是移植里最容易漏的语义。
//! 4. **bincode 纪律**：`.lmshapes` 部件用 bincode 位置编码。
//!    `ShapeKind` / `ShapeOp` 的变体**只能追加到末尾**，不得重排或删除；
//!    结构体增删字段必须同时提升部件版本（见 `lumino-project` 的 `LmshapesData`）。
//!    追加变体对旧文件是安全方向，重排则会让旧工程解出错误语义。
//! 5. **门长用拍而非 tick**：`ShapeStyle::gate_beats` 与 PPQ 无关，
//!    落地生成音符时再乘 PPQ（Spiderweb 的 `gate` 同样是拍）。

/// 形状允许的最高音高（含）——与 256 键口径一致
pub const SHAPE_KEY_MAX: u16 = 255;

/// 描边宽度下限（key）
pub const STROKE_WIDTH_MIN: u8 = 1;

/// 描边宽度上限（key）——与 `BrushConfig` 的 1..=20 一致
pub const STROKE_WIDTH_MAX: u8 = 20;

/// 颜色槽上限（0..=15，通道 10 留待后续开关）
pub const COLOUR_SLOT_MAX: u8 = 15;

/// 区域着色色号上限（16 = 通道 10；仅在工程启用通道 10 时可用）
pub const AREA_COLOUR_MAX: u8 = 16;

pub mod areas;
pub mod expr;
pub mod geometry;
pub mod model;
pub mod params;
pub mod pattern_presets;
pub mod style;
pub mod text;
pub mod velocity_env;

pub use areas::AreaSpot;
pub use expr::{Expr, ExprError};
pub use geometry::{ShapeAnchor, ShapeGeometry, ShapeKind, ShapeRect};
pub use model::{OpStage, ShapeModel, ShapeOp};
pub use params::{
    BetweenParams, BetweenRole, BetweenSet, ChopParams, ChopPiece, ClawMode, ClawParams,
    ClawPeriod, ClawTrash, ColourMode, CopyMode, CopyPathParams, CurveRef, CycleBy, CycleParams,
    DrawingRef, FlipMode, FlipWay, FunnelParams, FunnelStart, GateRangeParams, GlueParams,
    JoinedParams, LiveShapeParams, LiveWarning, LoopCurve, MaskMode, MaskParams, MaskShow,
    MergeParams, NestedParams, NestedWay, PathKind, PatternParams, PatternRef, PolyStyle,
    PolygonParams, RangeDir, RowRestart, Speed, StopsParams, StraightenParams, Stroke, StrokeKind,
    StrumParams, SustainParams, Sym, SymAxis, SymParams, TileBy, TileFlip, TileLayout, TilesParams,
    TumourGraphs, TumourParams, TumourShape, TumourSide, TumourWrap, factor_at,
};
pub use pattern_presets::{
    PATTERN_PRESETS, PatternPreset, pattern_from_preset, preset as pattern_preset,
};
pub use style::{ShapeFill, ShapeStyle, SpamAlign, SpamEnds};
pub use text::{DEFAULT_FONT, SUB_ROWS, TextAlign, TextLayoutMode, TextParams, TextUnit};
pub use velocity_env::{ENV_EPS, VelocityEnv, env_at, paint_env, tidy_env};
