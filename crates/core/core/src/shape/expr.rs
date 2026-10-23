//! 公式表达式求值（DRAW-016）
//!
//! 只做**数值求值**：编译成后缀序列，求值走显式栈。没有赋值、循环或任意代码执行。
//! 上限：源串 ≤4096 字符、括号嵌套 ≤64，非法输入一律 `Err`，不 panic 到调用方。
//!
//! 支持：`+ - * / % ^`、括号、一元负、常量 `pi`/`e`、变量 `x` 与白名单函数。
//! `min`/`max` 接受 1..N 个参数，`clamp(x, lo, hi)` 固定 3 个，其余函数按数学元数。

mod compile;
mod error;
mod eval;
mod func;

pub use compile::compile;
pub use error::ExprError;

/// 已编译公式（后缀序列）
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    ops: Vec<Op>,
}

#[derive(Debug, Clone, PartialEq)]
enum Op {
    Num(f64),
    Var(String),
    Neg,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Call(func::Func, usize),
}

#[cfg(test)]
mod tests;
