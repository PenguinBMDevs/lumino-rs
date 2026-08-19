//! 贴图瀑布流命令处理与渲染线程适配层
//!
//! 从宿主渲染线程（gfx runner/hires）迁入，通过
//! [`super::gpu_ctx::WaterfallGpuCtx`] 与宿主解耦：
//! 宿主只需提供 device/queue/texture_format 即可驱动贴图瀑布流
//! 的生成、流式上传、视口更新与视频导出上传。

mod common;
mod dirty;
mod drain;
mod generate;
mod regen;
mod video;
mod viewport;

pub use common::{ensure_renderer_for_config, push_waterfall_progress};
pub use dirty::handle_waterfall_dirty_overlay;
pub use drain::drain_waterfall_stream;
pub use generate::{WaterfallGenerateContext, handle_waterfall_dispose, handle_waterfall_generate};
pub use regen::handle_regenerate_waterfall_track;
pub use video::{WaterfallUploadTileParams, upload_waterfall_video_tiles};
pub use viewport::update_waterfall_viewport;
