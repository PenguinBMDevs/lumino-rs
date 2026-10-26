//! 数字框用的整数精确求值（DRAW-036）
//!
//! 与曲线公式的 [`super::Expr::eval`] 共用同一个解析器，但走独立的 `i64` 栈：
//! 整数字面量由 [`super::Op::Int`] 保留，浮点字面量、变量和函数一律拒绝。
//! 这样 `960*4` 不会先转 `f64` 再转回 `i64`，也不会把 `5/2` 静默截断成 2。

use super::{Expr, ExprError, Op};

/// 数字框整数求值入口：不带变量，结果必须落在 `i64` 内。
pub fn eval_int(src: &str) -> Result<i64, ExprError> {
    super::compile(src, &[])?.eval_int()
}

impl Expr {
    /// 按整数语义求值；仅支持整数字面量与 `+ - * / % ^`、括号、一元负。
    pub fn eval_int(&self) -> Result<i64, ExprError> {
        let mut stack: Vec<i64> = Vec::with_capacity(self.ops.len());
        for op in &self.ops {
            match op {
                Op::Int(value) => stack.push(*value),
                Op::Num(_) => return Err(ExprError::NonInteger),
                Op::Var(_) | Op::Call(_, _) => return Err(ExprError::NonInteger),
                Op::Neg => {
                    let value = pop(&mut stack)?;
                    stack.push(value.checked_neg().ok_or(ExprError::IntegerOverflow)?);
                }
                Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod | Op::Pow => {
                    let right = pop(&mut stack)?;
                    let left = pop(&mut stack)?;
                    let value = match op {
                        Op::Add => left.checked_add(right),
                        Op::Sub => left.checked_sub(right),
                        Op::Mul => left.checked_mul(right),
                        Op::Div => {
                            if right == 0 {
                                return Err(ExprError::DivisionByZero);
                            }
                            if left % right != 0 {
                                return Err(ExprError::NonInteger);
                            }
                            left.checked_div(right)
                        }
                        Op::Mod => {
                            if right == 0 {
                                return Err(ExprError::DivisionByZero);
                            }
                            left.checked_rem(right)
                        }
                        Op::Pow => Some(checked_pow(left, right)?),
                        _ => return Err(ExprError::Syntax("未知二元运算".into())),
                    };
                    stack.push(value.ok_or(ExprError::IntegerOverflow)?);
                }
            }
        }
        if stack.len() != 1 {
            return Err(ExprError::Syntax("公式没有唯一结果".into()));
        }
        Ok(stack[0])
    }
}

/// 防爆整数幂：指数必须为 `0..=64`，并且先做溢出预判再算。
///
/// 预判阈值沿用上游口径 `right * log2(|left|) > 1100`；`i64` 实际在 63 位就溢出，
/// 保留 1100 是刻意留出安全余量，避免不同平台的浮点边界差异。
pub fn checked_pow(left: i64, right: i64) -> Result<i64, ExprError> {
    if !(0..=64).contains(&right) {
        return Err(ExprError::IntegerExponent);
    }
    let abs = left.unsigned_abs();
    if abs > 1 && (right as f64) * (abs as f64).log2() > 1100.0 {
        return Err(ExprError::IntegerOverflow);
    }

    let mut result = 1_i64;
    let mut base = left;
    let mut exp = right as u32;
    while exp > 0 {
        if exp & 1 == 1 {
            result = result.checked_mul(base).ok_or(ExprError::IntegerOverflow)?;
        }
        exp >>= 1;
        if exp > 0 {
            base = base.checked_mul(base).ok_or(ExprError::IntegerOverflow)?;
        }
    }
    Ok(result)
}

/// 数字显示格式化：整数值不带小数点，`-0.0` 显示为 `0`。
pub fn fmt_number(value: f64) -> String {
    if value == 0.0 {
        return "0".to_string();
    }
    if value.is_finite()
        && value.fract() == 0.0
        && value >= i64::MIN as f64
        && value <= i64::MAX as f64
    {
        return (value as i64).to_string();
    }
    value.to_string()
}

fn pop(stack: &mut Vec<i64>) -> Result<i64, ExprError> {
    stack
        .pop()
        .ok_or_else(|| ExprError::Syntax("操作数不足".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_integer_exact() {
        assert_eq!(eval_int("960*4"), Ok(3840));
        assert_eq!(eval_int("(60+4)*16"), Ok(1024));
        assert_eq!(eval_int("7 % 4"), Ok(3));
        assert_eq!(eval_int("2^10"), Ok(1024));
        assert_eq!(eval_int("-2^2"), Ok(-4), "一元负低于幂");
    }

    #[test]
    fn test_rejects_non_integer() {
        for src in ["1.5", "x^2", "sin(pi/2)", "5/2", "1/0"] {
            assert!(eval_int(src).is_err(), "应拒绝非整数表达式：{src}");
        }
    }

    #[test]
    fn test_pow_limits_and_overflow() {
        assert_eq!(checked_pow(2, 0), Ok(1));
        assert_eq!(checked_pow(2, 62), Ok(1_i64 << 62));
        assert_eq!(checked_pow(2, 63), Err(ExprError::IntegerOverflow));
        assert_eq!(checked_pow(2, 64), Err(ExprError::IntegerOverflow));
        assert_eq!(checked_pow(9, 65), Err(ExprError::IntegerExponent));
        assert_eq!(checked_pow(9, -1), Err(ExprError::IntegerExponent));
        assert_eq!(checked_pow(9, 99_999_999), Err(ExprError::IntegerExponent));
        assert!(
            checked_pow(10, 64).is_err(),
            "预判/checked_mul 必须拒绝溢出"
        );
    }

    #[test]
    fn test_fmt_number() {
        assert_eq!(fmt_number(3840.0), "3840");
        assert_eq!(fmt_number(-0.0), "0");
        assert_eq!(fmt_number(1.5), "1.5");
        assert_eq!(fmt_number(0.25), "0.25");
    }
}
