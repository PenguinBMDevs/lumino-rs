/// 渲染上下文与帧状态、通信通道
pub mod context;
pub(crate) mod deferred;
pub(crate) mod onion_segments;
pub(crate) mod preview;
/// 渲染循环主运行逻辑
pub mod run;
pub(crate) mod video_export;

pub use context::{RenderContext, RenderFrameState, RenderThreadChannels};
pub use run::run_render_thread;
