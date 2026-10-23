//! 白名单函数与共享数值检查

use super::ExprError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Func {
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
    pub(super) fn from_name(name: &str) -> Option<Self> {
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

    pub(super) fn name(self) -> &'static str {
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

    pub(super) fn check_arity(self, got: usize) -> Result<(), ExprError> {
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

    pub(super) fn apply(self, args: &[f64]) -> Result<f64, ExprError> {
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

pub(super) fn finite(value: f64) -> Result<f64, ExprError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(ExprError::NotFinite)
    }
}

pub(super) fn pow(left: f64, right: f64) -> Result<f64, ExprError> {
    if !right.is_finite() || right.abs() > 64.0 {
        return Err(ExprError::ExponentTooLarge);
    }
    if left == 0.0 && right < 0.0 {
        return Err(ExprError::DivisionByZero);
    }
    finite(left.powf(right))
}
