use crate::epoch::GpuEpoch;

/// 渲染命令定义
pub mod commands;
/// Prepare 阶段（上传实例与 uniform）
pub mod prepare;
/// 渲染通道执行（现代绘制 pass）
pub mod render_pass;
/// 渲染循环 runner（帧调度入口）
pub mod runner;

/// 离屏纹理创建与复用
pub mod textures;

pub use runner::run_render_thread;

#[cfg(test)]
mod tests;

/// 渲染器对象集合（消除 prepare_renderers / execute_render_pass 的参数重复）
///
/// 将 6 个渲染器捆绑为一个结构体，使渲染管线函数签名更清晰。
pub struct Renderers {
    /// 背景网格渲染器（横向）
    pub grid: crate::GridRenderer,
    /// 纵向网格渲染器（转置版，键盘在底部，Key 范围明显分割）
    pub vertical_grid: crate::VerticalGridRenderer,
    /// 主音符渲染器
    pub note: crate::NoteRenderer,
    /// 洋葱皮渲染器（不透明背景层，在主音轨之前绘制）
    pub onion_skin: crate::NoteRenderer,
    /// 标尺渲染器
    pub ruler: crate::RulerRenderer,
    /// 走带（arrangement）渲染器
    pub arrangement: crate::ArrangementRenderer,
    /// CC 柱状条渲染器
    pub cc_bar: crate::CcBarRenderer,
    /// 洋葱皮常驻数据内容世代（`onion_skin` 常驻字节被改写时递增，单调 wrapping）。
    ///
    /// 契约：**任何改写 `onion_skin` 常驻字节的路径都必须经
    /// `onion_segments::bump_onion_epoch` 递增本计数**（全量会话完成、整轨
    /// TrackDelta、轨布局变更、区间删除、以及主音轨段内等长写/删/插）。
    /// 全局桶等派生索引据此判定重建——漏递增会让索引按旧
    /// `(key, start_tick)` 序取数（静默漏音/错位）。
    pub onion_epoch: u64,
    /// 创建时所属 GPU 代次（REND-019 PR-B 登记）。
    ///
    /// ⚠️ 与 `onion_epoch` 是**不同代次体系**：前者是内容代次（数据版本），
    /// 本字段是 device 代次（设备重建）——禁止混用（类型已隔离）。
    pub gpu_epoch: GpuEpoch,
}

impl Renderers {
    /// 创建默认渲染器（带 depth attachment，用于普通 UI 预览）。
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        epoch: GpuEpoch,
    ) -> Self {
        Self {
            grid: crate::GridRenderer::new(device, format),
            vertical_grid: crate::VerticalGridRenderer::new(device, format),
            note: crate::NoteRenderer::new(device, queue, format, epoch),
            onion_skin: crate::NoteRenderer::new_onion_skin(device, queue, format, epoch),
            ruler: crate::RulerRenderer::new(device, format),
            arrangement: crate::ArrangementRenderer::new(device, format),
            cc_bar: crate::CcBarRenderer::new(device, format),
            onion_epoch: 0,
            gpu_epoch: epoch,
        }
    }

    /// 创建视频导出专用渲染器（无 depth attachment，与无 depth 的 RenderPass 兼容）。
    ///
    /// 洋葱皮同样走无 depth 管线：钢琴卷帘导出分支会绘制洋葱皮渲染器，
    /// 管线携带 depth-stencil 状态会与无 depth 的 RenderPass 不兼容
    /// （见 `new_onion_skin_without_depth`）。
    pub fn new_for_video_export(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        epoch: GpuEpoch,
    ) -> Self {
        Self {
            grid: crate::GridRenderer::new_without_depth(device, format),
            vertical_grid: crate::VerticalGridRenderer::new_without_depth(device, format),
            note: crate::NoteRenderer::new_without_depth(device, queue, format, epoch),
            onion_skin: crate::NoteRenderer::new_onion_skin_without_depth(
                device, queue, format, epoch,
            ),
            ruler: crate::RulerRenderer::new_without_depth(device, format),
            arrangement: crate::ArrangementRenderer::new_without_depth(device, format),
            cc_bar: crate::CcBarRenderer::new_without_depth(device, format),
            onion_epoch: 0,
            gpu_epoch: epoch,
        }
    }
}
