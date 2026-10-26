//! 后缀表达式求值

use std::collections::BTreeMap;

use super::func::{finite, pow};
use super::{Expr, ExprError, Op};

impl Expr {
    /// 求值；`vars` 中缺少非 `x` 变量时返回 `Err`
    pub fn eval(&self, x: f64, vars: &BTreeMap<String, f64>) -> Result<f64, ExprError> {
        let mut stack: Vec<f64> = Vec::with_capacity(self.ops.len());
        for op in &self.ops {
            match op {
                Op::Num(value) => stack.push(*value),
                Op::Int(value) => stack.push(*value as f64),
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

fn pop(stack: &mut Vec<f64>) -> Result<f64, ExprError> {
    stack
        .pop()
        .ok_or_else(|| ExprError::Syntax("操作数不足".into()))
}
