//! 图片转 MIDI 的算法选项表（DRAW-030）
//!
//! 每个表项是 `(稳定 id, 显示名, i2m-rs 枚举)`：
//! - 显示名按当前界面语言实时翻译（切换语言后下拉项跟随）；
//! - 稳定 id 用于工程文件持久化，避免上游枚举增删导致旧工程错位。

use i2m_rs::{ColorIdMethod, KeyMode, NoteLengthMode, PaletteSource, ResizeAlgorithm};
use lumino_core::tr;

/// 混合模式（映射到 `PaletteSource`，不新增抖动实现）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum I2mBlend {
    /// 不抖动：按所选调色板算法直接匹配最近色
    None,
    /// 误差扩散（Floyd–Steinberg）
    Spread,
    /// 固定 4×4 网点（Bayer）
    Pattern,
}

/// 可选调色板算法（显示名 + `i2m-rs` `PaletteSource`）
///
/// 跳过 i2m-rs 中标记为"回退"的变体（`OnlyWpfMedianCut`、`Optics`）与
/// 纯初始化变体 `OnlyKMeansPlusPlus`，其余全部暴露给用户。
///
/// 显示名按当前界面语言实时翻译，故用函数而非静态表（静态表会把首次构建
/// 时的语言固化下来，切换语言后下拉项不跟随）。
pub fn palette_algorithms() -> Vec<(&'static str, &'static str, PaletteSource)> {
    vec![
        (
            "kmeans_plus_plus",
            "K-Means++",
            PaletteSource::KMeansPlusPlus,
        ),
        ("kmeans", "K-Means", PaletteSource::KMeans),
        ("popularity", tr!("流行色"), PaletteSource::Popularity),
        ("octree", tr!("八叉树"), PaletteSource::Octree),
        (
            "variance_split",
            tr!("方差分割"),
            PaletteSource::VarianceSplit,
        ),
        ("pca", tr!("PCA 主成分"), PaletteSource::Pca),
        ("max_min", "Max-Min", PaletteSource::MaxMin),
        (
            "native_kmeans",
            tr!("原生 K-Means"),
            PaletteSource::NativeKMeans,
        ),
        ("mean_shift", tr!("均值漂移"), PaletteSource::MeanShift),
        ("dbscan", tr!("DBSCAN 密度"), PaletteSource::Dbscan),
        ("gmm", tr!("GMM 混合模型"), PaletteSource::Gmm),
        ("hierarchical", tr!("层次聚类"), PaletteSource::Hierarchical),
        ("spectral", tr!("谱聚类"), PaletteSource::Spectral),
        ("lab_kmeans", "Lab K-Means", PaletteSource::LabKMeans),
        (
            "floyd_steinberg",
            tr!("Floyd-Steinberg 抖动"),
            PaletteSource::FloydSteinbergDither,
        ),
        (
            "ordered",
            tr!("Bayer 有序抖动"),
            PaletteSource::OrderedDither,
        ),
        (
            "fixed_bits",
            tr!("固定位深调色板"),
            PaletteSource::FixedBitPalette,
        ),
    ]
}

/// 可选 key 映射（稳定 id + 显示名 + `i2m-rs` `KeyMode`）
pub fn key_modes() -> Vec<(&'static str, &'static str, KeyMode)> {
    vec![
        ("all_keys", tr!("全部键"), KeyMode::AllKeys),
        (
            "white_filled",
            tr!("白键（填充）"),
            KeyMode::WhiteKeysFilled,
        ),
        (
            "white_clipped",
            tr!("白键（裁切）"),
            KeyMode::WhiteKeysClipped,
        ),
        (
            "white_fixed",
            tr!("白键（固定列）"),
            KeyMode::WhiteKeysFixed,
        ),
        (
            "black_filled",
            tr!("黑键（填充）"),
            KeyMode::BlackKeysFilled,
        ),
        (
            "black_clipped",
            tr!("黑键（裁切）"),
            KeyMode::BlackKeysClipped,
        ),
        (
            "black_fixed",
            tr!("黑键（固定列）"),
            KeyMode::BlackKeysFixed,
        ),
    ]
}

/// 可选音符长度策略（稳定 id + 显示名 + `i2m-rs` `NoteLengthMode`）
pub fn note_length_modes() -> Vec<(&'static str, &'static str, NoteLengthMode)> {
    vec![
        ("unlimited", tr!("不限"), NoteLengthMode::Unlimited),
        (
            "flow_with_color",
            tr!("同色内切分"),
            NoteLengthMode::FlowWithColor,
        ),
        (
            "split_to_grid",
            tr!("按网格切分"),
            NoteLengthMode::SplitToGrid,
        ),
    ]
}

/// 可选重采样算法（稳定 id + 显示名 + `i2m-rs` `ResizeAlgorithm`）
pub fn resize_algorithms() -> Vec<(&'static str, &'static str, ResizeAlgorithm)> {
    vec![
        (
            "area_resampling",
            tr!("面积重采样"),
            ResizeAlgorithm::AreaResampling,
        ),
        ("bilinear", "Bilinear", ResizeAlgorithm::Bilinear),
        ("nearest", tr!("最近邻"), ResizeAlgorithm::NearestNeighbor),
        ("bicubic", "Bicubic", ResizeAlgorithm::Bicubic),
        ("lanczos", "Lanczos-3", ResizeAlgorithm::Lanczos),
        ("gaussian", "Gaussian", ResizeAlgorithm::Gaussian),
        ("mitchell", "Mitchell", ResizeAlgorithm::Mitchell),
        ("box", tr!("盒式"), ResizeAlgorithm::BoxFilter),
        ("integral", tr!("积分图"), ResizeAlgorithm::IntegralImage),
        ("mode_pooling", "Mode", ResizeAlgorithm::ModePooling),
        ("hermite", "Hermite", ResizeAlgorithm::Hermite),
    ]
}

/// 可选颜色匹配口径（稳定 id + 显示名 + `i2m-rs` `ColorIdMethod`）
pub fn color_id_methods() -> Vec<(&'static str, &'static str, ColorIdMethod)> {
    vec![
        ("rgb", "RGB", ColorIdMethod::Rgb),
        ("hsv", "HSV", ColorIdMethod::Hsv),
        ("lab", "Lab（感知）", ColorIdMethod::Lab),
        ("ciede2000", "CIEDE2000", ColorIdMethod::Ciede2000),
        ("hsl", "HSL", ColorIdMethod::Hsl),
    ]
}

/// 可选混合模式（稳定 id + 显示名 + 语义）
pub fn blend_modes() -> Vec<(&'static str, &'static str, I2mBlend)> {
    vec![
        ("none", tr!("无（跟随调色板）"), I2mBlend::None),
        ("spread", tr!("扩散（误差）"), I2mBlend::Spread),
        ("pattern", tr!("网点（4×4）"), I2mBlend::Pattern),
    ]
}

/// 表内索引 → 稳定 id（越界回退默认 id）
pub fn id_at<T>(table: &[(&'static str, &'static str, T)], index: usize, fallback: &str) -> String {
    table
        .get(index)
        .map(|(id, _, _)| (*id).to_string())
        .unwrap_or_else(|| fallback.to_string())
}

/// 稳定 id → 表内索引（未知 id 回退 0）
pub fn index_of<T>(table: &[(&'static str, &'static str, T)], id: &str) -> usize {
    table.iter().position(|(key, _, _)| *key == id).unwrap_or(0)
}
