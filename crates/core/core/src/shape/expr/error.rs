//! 公式错误类型

use thiserror::Error;

/// 表达式错误
#[derive(Debug, Clone, PartialEq, Error)]
pub enum ExprError {
    /// 空表达式
    #[error("公式为空")]
    Empty,
    /// 超长
    #[error("公式超过 4096 字符")]
    TooLong,
    /// 嵌套过深
    #[error("公式嵌套超过 64 层")]
    TooDeep,
    /// 词法/语法错误
    #[error("公式语法错误：{0}")]
    Syntax(String),
    /// 未知变量
    #[error("未知变量：{0}")]
    UnknownName(String),
    /// 未知函数
    #[error("未知函数：{0}")]
    UnknownFunction(String),
    /// 函数参数个数错误
    #[error("函数 {name} 需要 {want} 个参数（实际 {got} 个）")]
    Arity {
        /// 函数名
        name: &'static str,
        /// 需要的参数个数（-1 表示至少 1 个）
        want: i32,
        /// 实际个数
        got: usize,
    },
    /// 除零
    #[error("公式除零")]
    DivisionByZero,
    /// 结果非有限值
    #[error("公式结果不是有限数")]
    NotFinite,
    /// 幂指数过大
    #[error("公式幂指数超过 64")]
    ExponentTooLarge,
    /// 求值时缺少变量
    #[error("求值缺少变量：{0}")]
    MissingValue(String),
}
