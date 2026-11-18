//! Tracy 性能剖析插桩（`tracy` feature；未启用时零开销）。
//!
//! 与 `lumino-gfx::profiling` 同一套宏语义（模块内无法跨 crate 复用 macro，
//! 保持 20 行重复优于引入公共 crate）。
//!
//! 使用约定：块级宏不要直接放进 `if` / `while` 条件——未启用 `tracy` feature
//! 时展开为块表达式，会触发 `clippy::blocks_in_conditions`；先绑定到变量再判断。

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

pub(crate) use tracy_span;
pub(crate) use tracy_zone;
