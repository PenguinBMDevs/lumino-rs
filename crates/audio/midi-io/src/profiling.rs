//! Tracy 性能剖析插桩（`tracy` feature）。
//!
//! 未启用 `tracy` feature 时宏展开为空操作，零开销；
//! 启用后若 Tracy 客户端未启动，`Client::running()` 返回 `None`，
//! 同样不发射任何数据，不会 panic。

/// 在 `$body` 执行期间发射一个 Tracy zone（作用域结束时自动结束）。
///
/// 用法：`crate::profiling::tracy_zone!("name", { ... })`
#[cfg(feature = "tracy")]
macro_rules! tracy_zone {
    ($name:literal, $body:block) => {{
        let _tracy_zone = tracy_client::Client::running()
            .map(|client| client.span(tracy_client::span_location!($name), 0));
        $body
    }};
}

/// 未启用 `tracy` feature 时的空操作版本（不产生任何代码）。
#[cfg(not(feature = "tracy"))]
macro_rules! tracy_zone {
    ($name:literal, $body:block) => {{
        $body
    }};
}

pub(crate) use tracy_zone;
