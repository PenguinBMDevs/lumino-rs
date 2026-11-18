//! Tracy 性能剖析插桩（`tracy` feature）。
//!
//! 未启用 feature 时，所有宏展开为原 block / 空语句，**编译期零开销**；
//! 启用后由根 crate 的 `tracy` feature 透传，构建命令：
//!
//! ```text
//! cargo build --profile fast-release --features tracy
//! ```
//!
//! 使用约定（与 `crates/audio/midi-io/src/profiling.rs` 一致）：
//! - 固定名字用字面量（`span_location!` / `plot_name!` 内联静态存储，无运行时分配）；
//! - zone 名字统一带模块前缀（`gfx.` / `app.` / `ffmpeg.`），Tracy 时间轴里可读。

/// 块级 zone：`$body` 的执行耗时计入同名 Tracy zone，并原样返回其值。
#[cfg(feature = "tracy")]
macro_rules! tracy_zone {
    ($name:literal, $body:block) => {{
        let _tracy_span = tracy_client::Client::running()
            .map(|client| client.span(tracy_client::span_location!($name), 0));
        $body
    }};
}

/// 未启用 Tracy：原样执行 `$body`。
#[cfg(not(feature = "tracy"))]
macro_rules! tracy_zone {
    ($name:literal, $body:block) => {
        $body
    };
}

/// 语句级 zone：在当前作用域构造 RAII guard，离开作用域时结束（用于整函数插桩）。
#[cfg(feature = "tracy")]
macro_rules! tracy_span {
    ($name:literal) => {
        let _tracy_span = tracy_client::Client::running()
            .map(|client| client.span(tracy_client::span_location!($name), 0));
    };
}

/// 未启用 Tracy：空语句。
#[cfg(not(feature = "tracy"))]
macro_rules! tracy_span {
    ($name:literal) => {};
}

/// 标量曲线（Tracy Plot）：`$value` 为任意可转 `f64` 的数值。
#[cfg(feature = "tracy")]
macro_rules! tracy_plot {
    ($name:literal, $value:expr) => {
        if let Some(client) = tracy_client::Client::running() {
            client.plot(tracy_client::plot_name!($name), ($value) as f64);
        }
    };
}

/// 未启用 Tracy：只对表达式求值一次（保持副作用语义，避免行为差异）。
#[cfg(not(feature = "tracy"))]
macro_rules! tracy_plot {
    ($name:literal, $value:expr) => {{
        let _ = &$value;
    }};
}

/// 标记一帧结束（Tracy Frames 视图）。未启用时为空操作。
#[inline]
pub(crate) fn frame_mark() {
    #[cfg(feature = "tracy")]
    tracy_client::frame_mark();
}

pub(crate) use tracy_plot;
pub(crate) use tracy_span;
pub(crate) use tracy_zone;
