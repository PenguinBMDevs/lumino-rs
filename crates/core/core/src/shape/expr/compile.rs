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
    let mut state = Shunt::new();

    for token in tokens {
        state.step(token, vars)?;
    }
    state.finish()
}

/// 调度场（shunting-yard）的可变状态：后缀输出、运算符栈与“下一个 token 期待什么”
struct Shunt {
    output: Vec<Op>,
    stack: Vec<Stack>,
    args: Vec<usize>,
    expect_operand: bool,
    depth: usize,
    previous_was_function: bool,
    last_was_lparen: bool,
    last_was_comma: bool,
}

impl Shunt {
    /// 初始状态：还没有输出，期待操作数
    fn new() -> Self {
        Self {
            output: Vec::new(),
            stack: Vec::new(),
            args: Vec::new(),
            expect_operand: true,
            depth: 0usize,
            previous_was_function: false,
            last_was_lparen: false,
            last_was_comma: false,
        }
    }

    /// 处理一个 token，推进一步
    fn step(&mut self, token: Token, vars: &[&str]) -> Result<(), ExprError> {
        match token {
            Token::Num(value) => {
                if !self.expect_operand {
                    return Err(ExprError::Syntax("数字前缺少运算符".into()));
                }
                self.output.push(Op::Num(value));
                self.expect_operand = false;
                self.previous_was_function = false;
                self.last_was_lparen = false;
                self.last_was_comma = false;
            }
            Token::Int(value) => {
                if !self.expect_operand {
                    return Err(ExprError::Syntax("数字前缺少运算符".into()));
                }
                self.output.push(Op::Int(value));
                self.expect_operand = false;
                self.previous_was_function = false;
                self.last_was_lparen = false;
                self.last_was_comma = false;
            }
            Token::Ident(name) => self.ident(name, vars)?,
            Token::LParen => self.lparen()?,
            Token::RParen => self.rparen()?,
            Token::Comma => self.comma()?,
            Token::Op(op) => self.op(op)?,
        }
        Ok(())
    }

    /// 标识符：函数压栈等括号，其余按常量/变量解析
    fn ident(&mut self, name: String, vars: &[&str]) -> Result<(), ExprError> {
        if !self.expect_operand {
            return Err(ExprError::Syntax("标识符前缺少运算符".into()));
        }
        if let Some(func) = Func::from_name(&name) {
            self.stack.push(Stack::Func(func));
            self.previous_was_function = true;
            return Ok(());
        }
        let op = match name.as_str() {
            "pi" => Op::Num(std::f64::consts::PI),
            "e" => Op::Num(std::f64::consts::E),
            "x" => Op::Var("x".into()),
            other if vars.contains(&other) => Op::Var(other.into()),
            other => return Err(ExprError::UnknownName(other.into())),
        };
        self.output.push(op);
        self.expect_operand = false;
        self.previous_was_function = false;
        self.last_was_lparen = false;
        self.last_was_comma = false;
        Ok(())
    }

    /// 左括号：累计嵌套深度、压栈，函数调用时开始记录参数个数
    fn lparen(&mut self) -> Result<(), ExprError> {
        self.depth += 1;
        if self.depth > 64 {
            return Err(ExprError::TooDeep);
        }
        self.stack.push(Stack::LParen);
        if self.previous_was_function {
            self.args.push(0);
        }
        self.expect_operand = true;
        self.previous_was_function = false;
        self.last_was_lparen = true;
        self.last_was_comma = false;
        Ok(())
    }

    /// 右括号：弹到左括号为止，函数标记存在时补一条调用指令
    fn rparen(&mut self) -> Result<(), ExprError> {
        self.depth = self.depth.saturating_sub(1);
        while !matches!(self.stack.last(), Some(Stack::LParen)) {
            let Some(item) = self.stack.pop() else {
                return Err(ExprError::Syntax("括号不匹配".into()));
            };
            emit(&mut self.output, item)?;
        }
        self.stack.pop();
        if matches!(self.stack.last(), Some(Stack::Func(_))) {
            let Some(Stack::Func(func)) = self.stack.pop() else {
                return Err(ExprError::Syntax("函数标记丢失".into()));
            };
            if self.last_was_comma {
                return Err(ExprError::Syntax("函数参数列表以逗号结尾".into()));
            }
            let commas = self.args.pop().unwrap_or(0);
            let count = if self.last_was_lparen { 0 } else { commas + 1 };
            func.check_arity(count)?;
            self.output.push(Op::Call(func, count));
        }
        self.expect_operand = false;
        self.previous_was_function = false;
        self.last_was_lparen = false;
        self.last_was_comma = false;
        Ok(())
    }

    /// 逗号：累加当前函数的参数个数，并弹到左括号为止
    fn comma(&mut self) -> Result<(), ExprError> {
        let Some(count) = self.args.last_mut() else {
            return Err(ExprError::Syntax("逗号出现在函数外".into()));
        };
        *count += 1;
        while !matches!(self.stack.last(), Some(Stack::LParen)) {
            let Some(item) = self.stack.pop() else {
                return Err(ExprError::Syntax("逗号出现在括号外".into()));
            };
            emit(&mut self.output, item)?;
        }
        self.expect_operand = true;
        self.previous_was_function = false;
        self.last_was_lparen = false;
        self.last_was_comma = true;
        Ok(())
    }

    /// 运算符：一元负/正先处理，否则按优先级弹出栈顶再压栈
    fn op(&mut self, op: OpKind) -> Result<(), ExprError> {
        if self.expect_operand {
            if op == OpKind::Sub {
                self.stack.push(Stack::Op(Op::Neg, 3));
            } else if op != OpKind::Add {
                return Err(ExprError::Syntax("缺少操作数".into()));
            }
        } else {
            let (prec, right_assoc) = precedence(op);
            while let Some(Stack::Op(_, top_prec)) = self.stack.last() {
                if *top_prec > prec || (*top_prec == prec && !right_assoc) {
                    let Some(item) = self.stack.pop() else {
                        break;
                    };
                    emit(&mut self.output, item)?;
                } else {
                    break;
                }
            }
            self.stack.push(Stack::Op(op.into(), prec));
            self.expect_operand = true;
        }
        self.previous_was_function = false;
        self.last_was_lparen = false;
        self.last_was_comma = false;
        Ok(())
    }

    /// 收尾：不能停在“等操作数”，栈里也不能剩左括号
    fn finish(mut self) -> Result<Expr, ExprError> {
        if self.expect_operand {
            return Err(ExprError::Syntax("公式以运算符结尾".into()));
        }
        while let Some(item) = self.stack.pop() {
            if matches!(item, Stack::LParen) {
                return Err(ExprError::Syntax("括号不匹配".into()));
            }
            emit(&mut self.output, item)?;
        }
        Ok(Expr { ops: self.output })
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
