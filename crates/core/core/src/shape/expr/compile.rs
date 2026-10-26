//! 公式编译：词法 → 调度场 → 后缀序列

use super::func::Func;
use super::{Expr, ExprError, Op};

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
            Token::Int(value) => {
                if !expect_operand {
                    return Err(ExprError::Syntax("数字前缺少运算符".into()));
                }
                output.push(Op::Int(value));
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

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Num(f64),
    Int(i64),
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
            // 纯十进制整数走 `Token::Int`，保证数字框 `960*4` 不经过 f64；
            // 带小数点/指数或超出 i64 的字面量仍走浮点路径。
            if !text.contains('.')
                && !text.contains('e')
                && !text.contains('E')
                && let Ok(value) = text.parse::<i64>()
            {
                out.push(Token::Int(value));
                continue;
            }
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
