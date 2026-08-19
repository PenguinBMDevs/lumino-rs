//! 批量编辑运算类型与输入解析
//!
//! 定义位于 core crate，以便 NoteStore 批量操作热路径和 UI 层共享同一类型，
//! 避免跨 crate 转换与重复实现。

/// 批量编辑运算类型
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BatchEditOperation {
    /// 绝对值赋值
    Set(f32),
    /// 加法
    Add(f32),
    /// 减法
    Subtract(f32),
    /// 乘法
    Multiply(f32),
    /// 除法
    Divide(f32),
}

impl BatchEditOperation {
    /// 将运算应用到基础值，返回新值
    ///
    /// - 乘除法结果向上取整
    /// - 其他运算按常规浮点计算
    pub fn apply(&self, base: f32) -> f32 {
        match self {
            Self::Set(v) => *v,
            Self::Add(v) => base + v,
            Self::Subtract(v) => base - v,
            Self::Multiply(v) => (base * v).ceil(),
            Self::Divide(v) => {
                if *v == 0.0 {
                    base
                } else {
                    (base / v).ceil()
                }
            }
        }
    }
}

/// 解析输入字符串为批量编辑运算
///
/// 支持格式：
/// - 无前缀数字：绝对值设置
/// - `+N`：加
/// - `-N`：减
/// - `*N`：乘
/// - `/N`：除
pub fn parse_batch_edit_input(input: &str) -> Option<BatchEditOperation> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }

    let first = input.chars().next()?;
    match first {
        '+' => {
            let value = input[1..].trim().parse::<f32>().ok()?;
            Some(BatchEditOperation::Add(value))
        }
        '-' => {
            let value = input[1..].trim().parse::<f32>().ok()?;
            Some(BatchEditOperation::Subtract(value))
        }
        '*' => {
            let value = input[1..].trim().parse::<f32>().ok()?;
            Some(BatchEditOperation::Multiply(value))
        }
        '/' => {
            let value = input[1..].trim().parse::<f32>().ok()?;
            if value == 0.0 {
                return None;
            }
            Some(BatchEditOperation::Divide(value))
        }
        _ => {
            let value = input.parse::<f32>().ok()?;
            Some(BatchEditOperation::Set(value))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty() {
        assert!(parse_batch_edit_input("").is_none());
        assert!(parse_batch_edit_input("   ").is_none());
    }

    #[test]
    fn test_parse_set() {
        let operation = parse_batch_edit_input("64").expect("输入 64 应可解析");
        assert_eq!(operation, BatchEditOperation::Set(64.0));
        assert!((operation.apply(10.0) - 64.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_parse_add() {
        let operation = parse_batch_edit_input("+20").expect("输入 +20 应可解析");
        assert_eq!(operation, BatchEditOperation::Add(20.0));
        assert!((operation.apply(50.0) - 70.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_parse_subtract() {
        let operation = parse_batch_edit_input("-15").expect("输入 -15 应可解析");
        assert_eq!(operation, BatchEditOperation::Subtract(15.0));
        assert!((operation.apply(50.0) - 35.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_parse_multiply_ceil() {
        let operation = parse_batch_edit_input("*1.5").expect("输入 *1.5 应可解析");
        assert_eq!(operation, BatchEditOperation::Multiply(1.5));
        assert!((operation.apply(10.0) - 15.0).abs() < f32::EPSILON);
        assert!((operation.apply(3.0) - 5.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_parse_divide_ceil() {
        let operation = parse_batch_edit_input("/2").expect("输入 /2 应可解析");
        assert_eq!(operation, BatchEditOperation::Divide(2.0));
        assert!((operation.apply(10.0) - 5.0).abs() < f32::EPSILON);
        assert!((operation.apply(7.0) - 4.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_parse_divide_by_zero() {
        assert!(parse_batch_edit_input("/0").is_none());
    }

    #[test]
    fn test_parse_invalid() {
        assert!(parse_batch_edit_input("abc").is_none());
        assert!(parse_batch_edit_input("+").is_none());
    }
}
