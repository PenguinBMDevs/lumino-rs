//! 通用解析辅助函数，集中消除对话框处理器中的重复数字校验逻辑。

/// 仅当字符串全部为 ASCII 数字时解析为 `u32`；空串或含非数字字符返回 `None`。
///
/// 与原先 `value.chars().all(|c| c.is_ascii_digit()) && let Ok(v) = value.parse::<u32>()`
/// 的行为完全一致（空串会通过 `chars().all` 检查，但 `parse` 失败，最终返回 `None`）。
pub fn parse_uint(s: &str) -> Option<u32> {
    if s.is_empty() || !s.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// 仅当字符串全部为 ASCII 数字且解析出的 `u8` 不超过 `max` 时返回该值。
///
/// 与原先 `value.parse::<u8>().is_ok_and(|v| v <= max)` 行为一致。
pub fn parse_u8_bounded(s: &str, max: u8) -> Option<u8> {
    if s.is_empty() || !s.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let v: u8 = s.parse().ok()?;
    (v <= max).then_some(v)
}

/// 字符串全部为 ASCII 数字，或为空。用于“允许数字或清空”的输入框校验。
pub fn is_digits_or_empty(s: &str) -> bool {
    s.is_empty() || s.chars().all(|c| c.is_ascii_digit())
}
