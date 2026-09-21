//! 钢琴瀑布流面板离屏渲染器（键盘底条 + 下落式音符）
//!
//! 将「下落式音符 + 底部钢琴键盘」绘制到一张离屏 `Rgba8Unorm` 纹理（`self.tex`），
//! 其纹理视图由 iced 的 `shader` 图元在自身渲染通道内**直接采样合成**（GPU→GPU），
//! 不经过 CPU 读回、不经 `image::Handle`、不进 iced 图集——因此与钢琴卷帘洋葱皮同样不闪烁。
//!
//! 关键约束（来自需求）：
//! - **音符数据禁止第二份拷贝**——直接 bind 渲染线程已持有的活体 GPU 音符实例缓冲
//!   （只读 `storage`），在面板 offscreen pass 中复用，不重新上传。
//! - **可视区间剔除**：通过一次 compute 预过滤，仅把落在可见纵轴区间的音符索引写入
//!   间接绘制缓冲，主绘制用 `draw_indirect` 只画可见音符——杜绝百万级音符每帧全量顶点。
//! - **背景透明**：整张纹理以 alpha=0 清屏（不填白），瀑布流区域透出面板自身背景；
//!   仅音符与键盘键位为不透明像素。

use std::sync::Arc;

use iced_wgpu::wgpu;

// 子模块
mod ensure;
mod instances;
mod renderer;
mod scene;
mod shaders;

/// 键盘渲染配色（0..1 线性）
pub struct KeyboardColors {
    /// 白键填充色
    pub white: [f32; 4],
    /// 黑键填充色
    pub black: [f32; 4],
}

impl KeyboardColors {
    /// 纯黑白配色（白键纯白，黑键纯黑），无边框/缝隙
    pub fn pure() -> Self {
        Self {
            white: [1.0, 1.0, 1.0, 1.0],
            black: [0.0, 0.0, 0.0, 1.0],
        }
    }
}

/// 键盘高度相对宽度的比例（面板宽度变化时高度联动）
///
/// 与视频导出渲染的瀑布流钢琴键盘保持一致：导出默认 1920×1080，
/// 键盘高 = 帧高 × 12% = 129.6px，键盘宽 = 帧宽 = 1920px，
/// → 高宽比 = 0.12 × (1080/1920) = 0.0675。
pub(crate) const KEY_HEIGHT_RATIO: f32 = 0.0675;
/// 键盘最小高度（像素）
pub(crate) const MIN_KEY_HEIGHT: f32 = 36.0;
/// 键盘最大高度（像素）
pub(crate) const MAX_KEY_HEIGHT: f32 = 140.0;
/// 面板内容内边距（用于计算键盘实际绘制宽度）
pub(crate) const PANEL_PADDING: f32 = 8.0;
/// compute 工作组大小
const WORKGROUP_SIZE: u32 = 64;
/// 活跃键颜色缓冲支持的最大键数（覆盖 128 与 256 键两种模式）
const KEY_COUNT_MAX: u32 = 256;
/// 每帧清零活跃键颜色用的零缓冲（KEY_COUNT_MAX × u32）
const ZERO_KEYCOLORS: [u8; (KEY_COUNT_MAX as usize) * 4] = [0u8; (KEY_COUNT_MAX as usize) * 4];

/// 单个实例数据：clip 空间矩形（xy=min, zw=size）+ 填充色 + 键号（索引活跃键颜色）
#[repr(C)]
struct Instance {
    rect: [f32; 4],
    color: [f32; 4],
    key: u32,
}

/// 音符 uniform（std140：vec2 对齐 8，其余顺排，总 32 字节）
#[repr(C)]
struct NoteUniforms {
    resolution: [f32; 2],
    zoom_x: f32,
    scroll_x: f32,
    current_track: u32,
    key_count: u32,
    keyboard_y: f32,
    _pad: f32,
}

/// 钢琴瀑布流离屏渲染器（持有管线与单位四边形，跨帧复用纹理/缓冲）
pub struct KeyboardRenderer {
    /// 键盘底条管线
    pipeline: wgpu::RenderPipeline,
    /// 单位四边形顶点缓冲
    quad_buffer: wgpu::Buffer,
    /// 下落式音符管线（间接绘制）
    note_pipeline: wgpu::RenderPipeline,
    /// 音符 bind group layout（只读 storage 实例 + uniform + 可见索引）
    note_bind_group_layout: wgpu::BindGroupLayout,
    /// 音符 uniform 缓冲（每帧 write_buffer 更新）
    uniform_buffer: wgpu::Buffer,
    /// 可视区间剔除 compute 管线
    cull_pipeline: wgpu::ComputePipeline,
    /// 剔除 bind group layout（notes + uniforms + visible_indices + draw_args）
    cull_bind_group_layout: wgpu::BindGroupLayout,
    /// 可见音符索引缓冲（storage，容量随音符数变化）
    visible_indices: wgpu::Buffer,
    /// 间接绘制参数缓冲 [vertex_count, instance_count, first_vertex, first_instance]
    draw_args: wgpu::Buffer,
    /// 活跃键颜色缓冲（storage，KEY_COUNT_MAX × u32，packed 0xRRGGBBAA；每帧清零后由 compute 写入）
    key_colors: Option<wgpu::Buffer>,
    /// 活跃键颜色 compute 管线
    keycolor_pipeline: wgpu::ComputePipeline,
    /// 活跃键颜色 compute bind group layout（notes + uniforms + key_colors + cull_offset）
    keycolor_bind_group_layout: wgpu::BindGroupLayout,
    /// 键盘着色器 bind group layout（key_colors storage read，仅顶点阶段）
    key_bgl: wgpu::BindGroupLayout,
    /// 离屏目标纹理（跨帧复用，尺寸变化才重建）；其视图交给 iced shader 图元直接采样合成
    tex: Option<wgpu::Texture>,
    /// `tex` 的视图（跨帧复用）；iced shader 图元持有其 `Arc` 克隆，在自身渲染通道内采样。
    /// 用 `Arc` 包裹以便跨帧/跨图元共享，且纹理重建时旧视图仍可被在途图元安全引用。
    tex_view: Option<Arc<wgpu::TextureView>>,
    /// 已分配的纹理/缓冲尺寸与音符数（用于判定是否需要重建）
    last_w: u32,
    last_h: u32,
    last_count: u32,
    /// 键盘实例缓冲（持久化复用）
    ///
    /// 键盘几何仅依赖 `width/height/key_count`，与滚动/缩放无关；旧实现每帧
    /// `create_buffer_init` 新建并立即 drop，触发驱动延迟释放，在播放自动滚动
    /// （scroll_x 每帧变化→签名变化→每帧 render_scene）时造成突发性 GPU 尖刺。
    /// 现跨帧复用，三者任一变化时才重建。
    instance_buffer: Option<wgpu::Buffer>,
    /// 实例缓冲对应的尺寸/键数（用于判定是否需要重建）
    inst_w: u32,
    inst_h: u32,
    inst_keys: u32,
}
