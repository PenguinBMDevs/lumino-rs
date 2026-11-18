//! Tracy 性能剖析插桩（`tracy` feature；未启用时零开销）。
//!
//! 与 `lumino-gfx::profiling` / `lumino-export::video::profiling` 同一套宏语义
//! （macro_rules 无法跨 crate 复用，保持小份重复优于引入公共 crate）。
//! 统一命名：`app.*`（Runner/导出线程）、`gfx.*`（渲染线程）、`ffmpeg.*`（管道）。

/// 块级 zone：`$body` 的执行耗时计入同名 Tracy zone，并原样返回其值。
#[cfg(feature = "tracy")]
macro_rules! tracy_zone {
    ($name:literal, $body:block) => {{
        let _tracy_span = tracy_client::Client::running()
            .map(|client| client.span(tracy_client::span_location!($name), 0));
        $body
    }};
}

#[cfg(not(feature = "tracy"))]
macro_rules! tracy_zone {
    ($name:literal, $body:block) => {
        $body
    };
}

/// 语句级 zone：当前作用域 RAII，离开作用域时结束。
#[cfg(feature = "tracy")]
macro_rules! tracy_span {
    ($name:literal) => {
        let _tracy_span = tracy_client::Client::running()
            .map(|client| client.span(tracy_client::span_location!($name), 0));
    };
}

#[cfg(not(feature = "tracy"))]
macro_rules! tracy_span {
    ($name:literal) => {};
}

/// 标量曲线（Tracy Plot）。
#[cfg(feature = "tracy")]
macro_rules! tracy_plot {
    ($name:literal, $value:expr) => {
        if let Some(client) = tracy_client::Client::running() {
            client.plot(tracy_client::plot_name!($name), ($value) as f64);
        }
    };
}

#[cfg(not(feature = "tracy"))]
macro_rules! tracy_plot {
    ($name:literal, $value:expr) => {{
        let _ = &$value;
    }};
}

pub(crate) use tracy_plot;
pub(crate) use tracy_span;
pub(crate) use tracy_zone;
