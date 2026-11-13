//! 自定义 Widget 辅助函数。
//!
//! 仅保留与 旧版 无关的纯函数；tooltip 封装已随 旧版 视图删除
//! （egui 侧 tooltip 由 `ui-egui` 自己提供）。

/// 解析并校验整数输入（供 PPQ 等编辑框共用）。
///
/// 与曲线公式共用 `lumino-core` 的表达式解析器；这里只负责把错误转成可直接
/// 显示给用户的文本。失败时调用方必须保留原输入，不得静默改成 0。
pub fn eval_int_field(src: &str, min: i64, max: i64) -> Result<i64, String> {
    let value = lumino_core::shape::expr::eval_int(src).map_err(|e| e.to_string())?;
    if !(min..=max).contains(&value) {
        return Err(format!("数值 {value} 超出允许范围 {min}..={max}"));
    }
    Ok(value)
}
