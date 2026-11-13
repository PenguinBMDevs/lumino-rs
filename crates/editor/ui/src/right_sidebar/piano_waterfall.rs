//! 右侧栏钢琴瀑布流预览面板（状态 + 占位）
//!
//! 画面由 `lumino-gfx` 离屏渲染成 wgpu 纹理，宿主把纹理注册为 egui 外部纹理
//! （`RENDER_PREVIEW_TEXTURE_ID` / `WATERFALL_TEXTURE_ID`），由 egui 直接采样
//! 铺放——不再经过 旧版 `shader` 图元，也不再需要 旧版 侧的合成通道。
//!
//! - 底部标准钢琴键盘（键数跟随全局 `enable_256key`：开启 256 键，否则 128 键）；
//! - 上方下落式音符瀑布流，复用渲染线程活体 GPU 实例缓冲（禁止第二份拷贝），
//!   配色与卷帘洋葱皮逐位一致，主音轨音符显示为蓝色，滚动与卷帘 X 缩放/滚动同步。
//!
//! 本文件只保留 **UI 无关状态**（离屏纹理视图 + 脏判断签名 + 目标尺寸）与
//! 旧版 侧的同宽占位；`waterfall_primitive.rs`（旧版 shader 图元）已删除。

pub(crate) mod key_layout;
pub(crate) mod keyboard_renderer;

use std::sync::Arc;

use wgpu;

/// 全屏瀑布流播放器的渲染状态。
pub(crate) struct WaterfallPlayerState {
    /// 离屏纹理视图（全屏瀑布流）
    pub view: Option<Arc<wgpu::TextureView>>,
    /// 脏判断签名（尺寸 / 键数 / 缩放 / 滚动 / 主音轨 / 音符数任一变化即重绘）
    pub cached_signature: Option<u64>,
    /// 缓存视图所属 GPU 代次（REND-019 PR-B：跨代次判废）
    pub(crate) cached_epoch: Option<lumino_gfx::GpuEpoch>,
    /// 主内容区实际像素尺寸（由 `responsive` 布局回调写入，供离屏渲染定尺寸）
    pub size: std::cell::RefCell<Option<(u32, u32)>>,
}

impl Default for WaterfallPlayerState {
    fn default() -> Self {
        Self {
            view: None,
            cached_signature: None,
            cached_epoch: None,
            size: std::cell::RefCell::new(None),
        }
    }
}
