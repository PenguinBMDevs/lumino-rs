//! 公式表达式求值（DRAW-016）
//!
//! 只做**数值求值**：编译成后缀序列，求值走显式栈。没有赋值、循环或任意代码执行。
//! 上限：源串 ≤4096 字符、括号嵌套 ≤64，非法输入一律 `Err`，不 panic 到调用方。
//!
//! 支持：`+ - * / % ^`、括号、一元负、常量 `pi`/`e`、变量 `x` 与白名单函数。
//! `min`/`max` 接受 1..N 个参数，`clamp(x, lo, hi)` 固定 3 个，其余函数按数学元数。

use std::collections::BTreeMap;

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
    Call(Func, usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Func {
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Atan2,
    Sinh,
    Cosh,
    Tanh,
    Sqrt,
    Exp,
    Ln,
    Log10,
    Log2,
    Abs,
    Floor,
    Ceil,
    Round,
    Sign,
    Min,
    Max,
    Clamp,
    Pow,
}

impl Func {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "sin" => Self::Sin,
            "cos" => Self::Cos,
            "tan" => Self::Tan,
            "asin" => Self::Asin,
            "acos" => Self::Acos,
            "atan" => Self::Atan,
            "atan2" => Self::Atan2,
            "sinh" => Self::Sinh,
            "cosh" => Self::Cosh,
            "tanh" => Self::Tanh,
            "sqrt" => Self::Sqrt,
            "exp" => Self::Exp,
            "ln" | "log" => Self::Ln,
            "log10" => Self::Log10,
            "log2" => Self::Log2,
            "abs" => Self::Abs,
            "floor" => Self::Floor,
            "ceil" => Self::Ceil,
            "round" => Self::Round,
            "sign" => Self::Sign,
            "min" => Self::Min,
            "max" => Self::Max,
            "clamp" => Self::Clamp,
            "pow" => Self::Pow,
            _ => return None,
        })
    }

    fn name(self) -> &'static str {
        match self {
            Self::Sin => "sin",
            Self::Cos => "cos",
            Self::Tan => "tan",
            Self::Asin => "asin",
            Self::Acos => "acos",
            Self::Atan => "atan",
            Self::Atan2 => "atan2",
            Self::Sinh => "sinh",
            Self::Cosh => "cosh",
            Self::Tanh => "tanh",
            Self::Sqrt => "sqrt",
            Self::Exp => "exp",
            Self::Ln => "ln",
            Self::Log10 => "log10",
            Self::Log2 => "log2",
            Self::Abs => "abs",
            Self::Floor => "floor",
            Self::Ceil => "ceil",
            Self::Round => "round",
            Self::Sign => "sign",
            Self::Min => "min",
            Self::Max => "max",
            Self::Clamp => "clamp",
            Self::Pow => "pow",
        }
    }

    fn check_arity(self, got: usize) -> Result<(), ExprError> {
        let want: i32 = match self {
            Self::Atan2 | Self::Pow => 2,
            Self::Clamp => 3,
            Self::Min | Self::Max => -1,
            _ => 1,
        };
        let ok = match want {
            -1 => got >= 1,
            n => got == n as usize,
        };
        if ok {
            Ok(())
        } else {
            Err(ExprError::Arity {
                name: self.name(),
                want,
                got,
            })
        }
    }

    fn apply(self, args: &[f64]) -> Result<f64, ExprError> {
        self.check_arity(args.len())?;
        let one = |x: f64| -> Result<f64, ExprError> {
            let v = match self {
                Self::Sin => x.sin(),
                Self::Cos => x.cos(),
                Self::Tan => x.tan(),
                Self::Asin => x.asin(),
                Self::Acos => x.acos(),
                Self::Atan => x.atan(),
                Self::Sinh => x.sinh(),
                Self::Cosh => x.cosh(),
                Self::Tanh => x.tanh(),
                Self::Sqrt => x.sqrt(),
                Self::Exp => x.exp(),
                Self::Ln | Self::Log10 | Self::Log2 => {
                    if x <= 0.0 {
                        return Err(ExprError::NotFinite);
                    }
                    match self {
                        Self::Log10 => x.log10(),
                        Self::Log2 => x.log2(),
                        _ => x.ln(),
                    }
                }
                Self::Abs => x.abs(),
                Self::Floor => x.floor(),
                Self::Ceil => x.ceil(),
                Self::Round => x.round(),
                Self::Sign => {
                    if x > 0.0 {
                        1.0
                    } else if x < 0.0 {
                        -1.0
                    } else {
                        0.0
                    }
                }
                _ => return Err(ExprError::NotFinite),
            };
            finite(v)
        };
        match self {
            Self::Atan2 => finite(args[0].atan2(args[1])),
            Self::Pow => pow(args[0], args[1]),
            Self::Min => finite(args.iter().copied().fold(f64::INFINITY, f64::min)),
            Self::Max => finite(args.iter().copied().fold(f64::NEG_INFINITY, f64::max)),
            Self::Clamp => {
                if !args[1].is_finite() || !args[2].is_finite() || args[1] > args[2] {
                    return Err(ExprError::NotFinite);
                }
                finite(args[0].clamp(args[1], args[2]))
            }
            _ => one(args[0]),
        }
    }
}

/// 编译公式；`vars` 是除 `x` 外允许出现的变量名
pub fn compile(src: &str, vars: &[&str]) -> Result<Expr, ExprError> {
    if src.trim().is_empty() {
        return Err(ExprError::Empty);
    }
    if src.chars().count() > 4096 {
        return Err(ExprError::TooLong);
    }
    let tokens = tokenize(src)?;
    if tokens.is_empty() {
        return Err(ExprError::Empty);
    }
    let mut output = Vec::new();
    let mut stack: Vec<Stack> = Vec::new();
    let mut args: Vec<usize> = Vec::new();
    let mut expect_operand = true;
    let mut depth = 0usize;
    let mut previous_was_function = false;
    let mut last_was_lparen = false;
    let mut last_was_comma = false;

    for token in tokens {
        match token {
            Token::Num(value) => {
                if !expect_operand {
                    return Err(ExprError::Syntax("数字前缺少运算符".into()));
                }
                output.push(Op::Num(value));
                expect_operand = false;
                previous_was_function = false;
                last_was_lparen = false;
                last_was_comma = false;
            }
            Token::Ident(name) => {
                if !expect_operand {
                    return Err(ExprError::Syntax("标识符前缺少运算符".into()));
                }
                if let Some(func) = Func::from_name(&name) {
                    stack.push(Stack::Func(func));
                    previous_was_function = true;
                    continue;
                }
                let op = match name.as_str() {
                    "pi" => Op::Num(std::f64::consts::PI),
                    "e" => Op::Num(std::f64::consts::E),
                    "x" => Op::Var("x".into()),
                    other if vars.contains(&other) => Op::Var(other.into()),
                    other => return Err(ExprError::UnknownName(other.into())),
                };
                output.push(op);
                expect_operand = false;
                previous_was_function = false;
                last_was_lparen = false;
                last_was_comma = false;
            }
            Token::LParen => {
                depth += 1;
                if depth > 64 {
                    return Err(ExprError::TooDeep);
                }
                stack.push(Stack::LParen);
                if previous_was_function {
                    args.push(0);
                }
                expect_operand = true;
                previous_was_function = false;
                last_was_lparen = true;
                last_was_comma = false;
            }
            Token::RParen => {
                depth = depth.saturating_sub(1);
                while !matches!(stack.last(), Some(Stack::LParen)) {
                    let Some(item) = stack.pop() else {
                        return Err(ExprError::Syntax("括号不匹配".into()));
                    };
                    emit(&mut output, item)?;
                }
                stack.pop();
                if matches!(stack.last(), Some(Stack::Func(_))) {
                    let Some(Stack::Func(func)) = stack.pop() else {
                        return Err(ExprError::Syntax("函数标记丢失".into()));
                    };
                    if last_was_comma {
                        return Err(ExprError::Syntax("函数参数列表以逗号结尾".into()));
                    }
                    let commas = args.pop().unwrap_or(0);
                    let count = if last_was_lparen { 0 } else { commas + 1 };
                    func.check_arity(count)?;
                    output.push(Op::Call(func, count));
                }
                expect_operand = false;
                previous_was_function = false;
                last_was_lparen = false;
                last_was_comma = false;
            }
            Token::Comma => {
                let Some(count) = args.last_mut() else {
                    return Err(ExprError::Syntax("逗号出现在函数外".into()));
                };
                *count += 1;
                while !matches!(stack.last(), Some(Stack::LParen)) {
                    let Some(item) = stack.pop() else {
                        return Err(ExprError::Syntax("逗号出现在括号外".into()));
                    };
                    emit(&mut output, item)?;
                }
                expect_operand = true;
                previous_was_function = false;
                last_was_lparen = false;
                last_was_comma = true;
            }
            Token::Op(op) => {
                if expect_operand {
                    if op == OpKind::Sub {
                        stack.push(Stack::Op(Op::Neg, 3));
                    } else if op != OpKind::Add {
                        return Err(ExprError::Syntax("缺少操作数".into()));
                    }
                } else {
                    let (prec, right_assoc) = precedence(op);
                    while let Some(Stack::Op(_, top_prec)) = stack.last() {
                        if *top_prec > prec || (*top_prec == prec && !right_assoc) {
                            let Some(item) = stack.pop() else {
                                break;
                            };
                            emit(&mut output, item)?;
                        } else {
                            break;
                        }
                    }
                    stack.push(Stack::Op(op.into(), prec));
                    expect_operand = true;
                }
                previous_was_function = false;
                last_was_lparen = false;
                last_was_comma = false;
            }
        }
    }
    if expect_operand {
        return Err(ExprError::Syntax("公式以运算符结尾".into()));
    }
    while let Some(item) = stack.pop() {
        if matches!(item, Stack::LParen) {
            return Err(ExprError::Syntax("括号不匹配".into()));
        }
        emit(&mut output, item)?;
    }
    Ok(Expr { ops: output })
}

impl Expr {
    /// 求值；`vars` 中缺少非 `x` 变量时返回 `Err`
    pub fn eval(&self, x: f64, vars: &BTreeMap<String, f64>) -> Result<f64, ExprError> {
        let mut stack: Vec<f64> = Vec::with_capacity(self.ops.len());
        for op in &self.ops {
            match op {
                Op::Num(value) => stack.push(*value),
                Op::Var(name) => {
                    let value = if name == "x" {
                        Some(x)
                    } else {
                        vars.get(name).copied()
                    };
                    stack.push(value.ok_or_else(|| ExprError::MissingValue(name.clone()))?);
                }
                Op::Neg => {
                    let value = pop(&mut stack)?;
                    stack.push(-value);
                }
                Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod | Op::Pow => {
                    let right = pop(&mut stack)?;
                    let left = pop(&mut stack)?;
                    let value = match op {
                        Op::Add => left + right,
                        Op::Sub => left - right,
                        Op::Mul => left * right,
                        Op::Div => {
                            if right == 0.0 {
                                return Err(ExprError::DivisionByZero);
                            }
                            left / right
                        }
                        Op::Mod => {
                            if right == 0.0 {
                                return Err(ExprError::DivisionByZero);
                            }
                            left % right
                        }
                        Op::Pow => pow(left, right)?,
                        _ => return Err(ExprError::Syntax("未知二元运算".into())),
                    };
                    stack.push(finite(value)?);
                }
                Op::Call(func, count) => {
                    if stack.len() < *count {
                        return Err(ExprError::Syntax("函数参数不足".into()));
                    }
                    let start = stack.len() - *count;
                    let args = stack.split_off(start);
                    stack.push(func.apply(&args)?);
                }
            }
        }
        if stack.len() != 1 {
            return Err(ExprError::Syntax("公式没有唯一结果".into()));
        }
        finite(stack[0])
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Stack {
    Op(Op, u8),
    Func(Func),
    LParen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpKind {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
}

impl From<OpKind> for Op {
    fn from(kind: OpKind) -> Self {
        match kind {
            OpKind::Add => Self::Add,
            OpKind::Sub => Self::Sub,
            OpKind::Mul => Self::Mul,
            OpKind::Div => Self::Div,
            OpKind::Mod => Self::Mod,
            OpKind::Pow => Self::Pow,
        }
    }
}

fn precedence(op: OpKind) -> (u8, bool) {
    match op {
        OpKind::Add | OpKind::Sub => (1, false),
        OpKind::Mul | OpKind::Div | OpKind::Mod => (2, false),
        OpKind::Pow => (4, true),
    }
}

fn emit(output: &mut Vec<Op>, item: Stack) -> Result<(), ExprError> {
    match item {
        Stack::Op(op, _) => output.push(op),
        Stack::LParen => return Err(ExprError::Syntax("括号不匹配".into())),
        Stack::Func(func) => {
            return Err(ExprError::Syntax(format!("函数 {} 缺少括号", func.name())));
        }
    }
    Ok(())
}

fn pop(stack: &mut Vec<f64>) -> Result<f64, ExprError> {
    stack
        .pop()
        .ok_or_else(|| ExprError::Syntax("操作数不足".into()))
}

fn finite(value: f64) -> Result<f64, ExprError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(ExprError::NotFinite)
    }
}

fn pow(left: f64, right: f64) -> Result<f64, ExprError> {
    if !right.is_finite() || right.abs() > 64.0 {
        return Err(ExprError::ExponentTooLarge);
    }
    if left == 0.0 && right < 0.0 {
        return Err(ExprError::DivisionByZero);
    }
    finite(left.powf(right))
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Num(f64),
    Ident(String),
    LParen,
    RParen,
    Comma,
    Op(OpKind),
}

fn tokenize(src: &str) -> Result<Vec<Token>, ExprError> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        let ch = chars[i];
        if ch.is_whitespace() {
            i += 1;
            continue;
        }
        if ch.is_ascii_digit() || (ch == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit))
        {
            let start = i;
            i += 1;
            while i < chars.len()
                && (chars[i].is_ascii_digit()
                    || chars[i] == '.'
                    || chars[i] == 'e'
                    || chars[i] == 'E'
                    || ((chars[i] == '+' || chars[i] == '-')
                        && matches!(chars.get(i.wrapping_sub(1)), Some(&('e' | 'E')))))
            {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            let value = text
                .parse::<f64>()
                .map_err(|_| ExprError::Syntax(format!("非法数字：{text}")))?;
            out.push(Token::Num(value));
            continue;
        }
        if ch.is_alphabetic() || ch == '_' {
            let start = i;
            i += 1;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            out.push(Token::Ident(chars[start..i].iter().collect()));
            continue;
        }
        match ch {
            '(' => out.push(Token::LParen),
            ')' => out.push(Token::RParen),
            ',' => out.push(Token::Comma),
            '+' => out.push(Token::Op(OpKind::Add)),
            '-' => out.push(Token::Op(OpKind::Sub)),
            '*' => {
                if chars.get(i + 1) == Some(&'*') {
                    out.push(Token::Op(OpKind::Pow));
                    i += 1;
                } else {
                    out.push(Token::Op(OpKind::Mul));
                }
            }
            '×' => out.push(Token::Op(OpKind::Mul)),
            '/' => out.push(Token::Op(OpKind::Div)),
            '%' => out.push(Token::Op(OpKind::Mod)),
            '^' => out.push(Token::Op(OpKind::Pow)),
            _ => {
                return Err(ExprError::Syntax(format!("未知字符：{ch}")));
            }
        }
        i += 1;
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
