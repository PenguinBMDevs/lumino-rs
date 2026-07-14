pub mod commands;
pub mod prepare;
pub mod render_pass;
pub mod runner;

pub mod textures;

pub use runner::run_render_thread;

/// 渲染器对象集合（消除 prepare_renderers / execute_render_pass 的参数重复）
///
/// 将 5 个渲染器捆绑为一个结构体，使渲染管线函数签名更清晰。
pub struct Renderers {
    pub grid: crate::GridRenderer,
    pub note: crate::NoteRenderer,
    pub ruler: crate::RulerRenderer,
    pub arrangement: crate::ArrangementRenderer,
    pub cc_bar: crate::CcBarRenderer,
}
