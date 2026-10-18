//! 通道模式与同键重叠消解（S0-4）
//!
//! 移植自 Spiderweb 的 Channels 口径（`scripts/lang/en/help/channels.txt`）：
//!
//! - **As drawn（[`ChannelMode::Raw`]）**：音符保持形状生成时的原样，同一音高可以重叠
//!   ——这是黑乐谱纹理（肿瘤、爪机、扫弦、公式图案叠出来的密集同键音）的前提。
//! - **Single channel（[`ChannelMode::Single`]）**：同键重叠被消解为不重叠：
//!   1. **同 tick 同键**合并为一条：取**最响**力度、**最长**长度（Spiderweb：the loudest, as long as the longest）；
//!   2. **不同起点但重叠**：前者在后者起点处**截断**，后者**延伸到前者的原终点**
//!      （后者本来更长时保留更长的那端）。
//!
//! ## 边界约定
//!
//! - 只按 **(key, tick)** 组织：不同音高互不影响；
//! - 截断/延伸结果不足 [`MIN_NOTE_TICKS`] 时抬到该下限（0 长音符在渲染与播放上都无意义）；
//! - 结果与输入顺序无关（内部按 `key → tick → 原终点降序` 稳定排序后处理）；
//! - 本模块**不改通道号**：`Multi channel` 的自动分配属于后续任务，这里只做重叠消解。

use crate::note::Note;

/// 音符最短长度（tick）：截断后不足此值的抬到该值
pub const MIN_NOTE_TICKS: f32 = 1.0;

/// 通道模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChannelMode {
    /// 原样保留（允许同键重叠）
    #[default]
    Raw,
    /// 单通道：消解同键重叠
    Single,
}

impl ChannelMode {
    /// 稳定标识（配置/日志用）
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Single => "single",
        }
    }

    /// 从标识解析（未知值回退默认 `Raw`）
    pub fn from_str_lossy(value: &str) -> Self {
        match value {
            "single" => Self::Single,
            _ => Self::Raw,
        }
    }
}

/// 按模式处理音符列表，返回**被改动的音符数**（截断 / 延伸 / 合并）
///
/// `Raw` 为无操作（返回 0）。函数对列表做原地修改，**不排序**输入本身
/// （内部按 `(key, tick)` 分组处理，顺序对外不可见）。
pub fn apply_channel_mode(notes: &mut Vec<Note>, mode: ChannelMode) -> usize {
    match mode {
        ChannelMode::Raw => 0,
        ChannelMode::Single => resolve_single_channel(notes),
    }
}

/// 单通道消解：同键重叠截断/延伸 + 同 tick 合并
///
/// 返回被改动的音符数（合并掉的音符也计入）。
pub fn resolve_single_channel(notes: &mut Vec<Note>) -> usize {
    if notes.len() < 2 {
        return 0;
    }
    // 按音高分组，组内按 (tick 升序, 原终点降序) 稳定排序：
    // 「原终点降序」保证同 tick 时最长的那条先入结果，合并语义与顺序无关。
    let mut order: Vec<usize> = (0..notes.len()).collect();
    order.sort_by(|&a, &b| {
        let (ka, kb) = (notes[a].key, notes[b].key);
        ka.cmp(&kb).then_with(|| {
            notes[a]
                .tick
                .partial_cmp(&notes[b].tick)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    let ea = notes[a].tick + notes[a].length;
                    let eb = notes[b].tick + notes[b].length;
                    eb.partial_cmp(&ea).unwrap_or(std::cmp::Ordering::Equal)
                })
        })
    });

    let mut changed = 0usize;
    let mut removed: Vec<bool> = vec![false; notes.len()];
    // 每组（同 key）处理：`prev` 为结果中该键最后一条的索引
    let mut prev: Option<usize> = None;
    for &idx in &order {
        let Some(prev_idx) = prev else {
            prev = Some(idx);
            continue;
        };
        let prev_key = notes[prev_idx].key;
        if notes[idx].key != prev_key {
            prev = Some(idx);
            continue;
        }
        let prev_start = notes[prev_idx].tick;
        let prev_end = prev_start + notes[prev_idx].length;
        let cur_start = notes[idx].tick;
        let cur_end = cur_start + notes[idx].length;

        if cur_start == prev_start {
            // ① 同 tick 合并：最响 + 最长
            if notes[idx].velocity > notes[prev_idx].velocity {
                notes[prev_idx].velocity = notes[idx].velocity;
                changed += 1;
            }
            if cur_end > prev_end {
                notes[prev_idx].length = cur_end - prev_start;
                changed += 1;
            }
            removed[idx] = true;
            changed += 1;
            continue;
        }

        if cur_start < prev_end {
            // ② 重叠：前者截断到后者起点，后者延伸到前者原终点（更长则保留更长）
            let cut_len = (cur_start - prev_start).max(MIN_NOTE_TICKS);
            if (notes[prev_idx].length - cut_len).abs() > f32::EPSILON {
                notes[prev_idx].length = cut_len;
                changed += 1;
            }
            let stretched = (prev_end.max(cur_end) - cur_start).max(MIN_NOTE_TICKS);
            if (notes[idx].length - stretched).abs() > f32::EPSILON {
                notes[idx].length = stretched;
                changed += 1;
            }
        }
        prev = Some(idx);
    }

    if removed.iter().any(|&r| r) {
        let mut k = 0usize;
        notes.retain(|_| {
            let keep = !removed[k];
            k += 1;
            keep
        });
    }
    changed
}

#[cfg(test)]
mod tests;
