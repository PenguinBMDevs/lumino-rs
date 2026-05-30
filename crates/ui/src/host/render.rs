//! Host 渲染子模块 - 处理 UI、网格和音符渲染
//!
//! 支持三种渲染模式：
//! 1. 单线程模式：UI更新和WGPU渲染在同一个线程
//! 2. 多线程模式（旧）：WGPU渲染在独立线程，UI线程只生成渲染命令
//! 3. 分离渲染模式（新）：UI线程和WGPU渲染线程完全分离，零拷贝数据共享
//!
//! 子模块组织：
//! - `data`: 渲染数据类型定义
//! - `frame`: 帧准备逻辑（FPS计算、播放状态）
//! - `grid`: 网格、键盘、标尺生成
//! - `notes`: 音符实例更新
//! - `viewport`: 视口信息收集
//! - `encoder`: 渲染编码器管理
//! - `single_thread`: 单线程渲染模式
//! - `separate_thread`: 分离渲染线程模式
//! - `ui`: iced UI 渲染

use iced_wgpu::wgpu;

use crate::host::Host;

// 子模块声明
mod data;
mod encoder;
mod frame;
mod grid;
pub(super) mod note_worker;
mod notes;
mod separate_thread;
mod single_thread;
mod ui;
mod viewport;

// 公开子模块的公共类型

// =============================================================================
// 常量定义 - 避免魔法数字
// =============================================================================

/// 默认键盘宽度（像素）
pub const DEFAULT_KEYBOARD_WIDTH: f32 = 60.0;
/// 默认标尺高度（像素）
pub const DEFAULT_RULER_HEIGHT: f32 = 30.0;
/// 每小节 tick 数（4/4拍，480 PPQ）
pub const TICKS_PER_MEASURE: u32 = 1920;
/// 每拍 tick 数（480 PPQ）
pub const TICKS_PER_BEAT: u32 = 480;
/// 一个八度内的音符数
pub const NOTES_PER_OCTAVE: isize = 12;
/// FPS 更新间隔（毫秒）
pub const FPS_UPDATE_INTERVAL_MS: u128 = 50;

impl Host {
    /// 判断指定琴键是否为黑键
    ///
    /// 钢琴键盘布局：每个八度有12个键，其中黑键位于第1, 3, 6, 8, 10位
    ///（以C大调为例：C(白), C#(黑), D(白), D#(黑), E(白), F(白), F#(黑), G(白), G#(黑), A(白), A#(黑), B(白)）
    #[inline]
    pub(super) fn is_black_key(key_index: isize) -> bool {
        let note_in_octave = key_index.rem_euclid(NOTES_PER_OCTAVE);
        matches!(note_in_octave, 1 | 3 | 6 | 8 | 10)
    }

    /// 主渲染入口
    ///
    /// 根据配置选择渲染模式：
    /// - 单线程模式：直接在当前线程执行所有渲染
    /// - 多线程模式（旧）：发送渲染命令到独立渲染线程
    /// - 分离渲染模式（新）：UI线程只更新数据，WGPU线程独立渲染
    pub fn redraw_requested(
        &mut self,
        frame: &wgpu::SurfaceTexture,
        view: &wgpu::TextureView,
        gfx: &lumino_gfx::Context,
    ) {
        // 通知 puffin 新的一帧开始 - 必须在 profile_function 之前调用
        puffin::GlobalProfiler::lock().new_frame();

        puffin::profile_function!();

        // 帧准备：更新 FPS 和播放状态
        self.process_frame_preparation();

        // 更新光标位置（用于音符预览）
        self.update_cursor_for_preview();

        // 根据渲染模式选择不同的渲染路径
        if self.render_ctx.use_separate_render_thread {
            self.render_with_separate_thread(frame, gfx);
        } else {
            self.render_single_thread(frame, view, gfx);
        }
    }

    /// 清除 UI 缓存以强制重绘
    #[inline]
    pub(crate) fn clear_cache(&mut self) {
        self.render_ctx.cache = std::mem::take(&mut self.render_ctx.cache);
    }
}
