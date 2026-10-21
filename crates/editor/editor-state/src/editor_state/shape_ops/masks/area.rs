//! Mask 区域代数：逐 key 行的区间并/差/交 + 圆刷带宽 + 闭合轮廓内部
//!
//! 所有区间都以 tick 表示；`Area` 恒按 `(key, start)` 排序并合并。

use crate::ShapeNote;

const EPS: f32 = 1e-6;

/// 一个 key 行上的一段区间
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RowSpan {
    /// key 行
    pub key: u16,
    /// 起点（tick）
    pub start: f32,
    /// 终点（tick）
    pub end: f32,
}

/// 遮罩区域：已排序、已合并的逐行区间集合
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Area {
    spans: Vec<RowSpan>,
}

impl Area {
    /// 从任意区间构造（自动排序、合并、丢弃空/非有限段）
    pub fn new(spans: impl IntoIterator<Item = RowSpan>) -> Self {
        let mut spans: Vec<RowSpan> = spans
            .into_iter()
            .filter(|span| {
                span.start.is_finite() && span.end.is_finite() && span.end > span.start + EPS
            })
            .collect();
        spans.sort_by(|a, b| {
            a.key.cmp(&b.key).then(
                a.start
                    .partial_cmp(&b.start)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
        });
        let mut out: Vec<RowSpan> = Vec::with_capacity(spans.len());
        for span in spans {
            if let Some(last) = out.last_mut()
                && last.key == span.key
                && span.start <= last.end + EPS
            {
                last.end = last.end.max(span.end);
            } else {
                out.push(span);
            }
        }
        Self { spans: out }
    }

    /// 由音符的原始跨度构造（不做带宽）
    pub fn from_notes(notes: &[ShapeNote]) -> Self {
        Self::new(notes.iter().filter_map(|note| {
            let start = note.tick;
            let end = start + note.length.max(0.0);
            (end > start + EPS).then_some(RowSpan {
                key: note.key,
                start,
                end,
            })
        }))
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    /// 区间段数
    pub fn len(&self) -> usize {
        self.spans.len()
    }

    /// 只读段列表
    pub fn spans(&self) -> &[RowSpan] {
        &self.spans
    }

    /// 某个 key 行上的段（已排序）
    pub fn spans_on(&self, key: u16) -> &[RowSpan] {
        let start = self.spans.partition_point(|span| span.key < key);
        let end = self.spans.partition_point(|span| span.key <= key);
        &self.spans[start..end]
    }

    /// 并集
    pub fn joined(&self, other: &Self) -> Self {
        let mut spans = self.spans.clone();
        spans.extend_from_slice(&other.spans);
        Self::new(spans)
    }

    /// 差集：`self - other`
    pub fn minus(&self, other: &Self) -> Self {
        let mut out = Vec::new();
        for span in &self.spans {
            let mut cursor = span.start;
            for cut in other.spans_on(span.key) {
                if cut.end <= cursor + EPS {
                    continue;
                }
                if cut.start >= span.end - EPS {
                    break;
                }
                if cut.start > cursor + EPS {
                    out.push(RowSpan {
                        key: span.key,
                        start: cursor,
                        end: cut.start.min(span.end),
                    });
                }
                cursor = cursor.max(cut.end);
                if cursor >= span.end - EPS {
                    break;
                }
            }
            if cursor < span.end - EPS {
                out.push(RowSpan {
                    key: span.key,
                    start: cursor,
                    end: span.end,
                });
            }
        }
        Self::new(out)
    }

    /// 交集
    pub fn common(&self, other: &Self) -> Self {
        let mut out = Vec::new();
        for span in &self.spans {
            for cut in other.spans_on(span.key) {
                let start = span.start.max(cut.start);
                let end = span.end.min(cut.end);
                if end > start + EPS {
                    out.push(RowSpan {
                        key: span.key,
                        start,
                        end,
                    });
                }
            }
        }
        Self::new(out)
    }

    /// 是否与 `[start, end)` 在 `key` 行相交
    pub fn intersects(&self, key: u16, start: f32, end: f32) -> bool {
        self.spans_on(key)
            .iter()
            .any(|span| span.end > start + EPS && span.start < end - EPS)
    }
}

/// 计算 `base_areas` 的输入
pub struct MaskAreaInput<'a> {
    /// 遮罩自身音符
    pub notes: &'a [ShapeNote],
    /// 闭合轮廓（逻辑 `(tick, key)`；无 = 开放线）
    pub polygon: Option<&'a [(f32, f32)]>,
    /// Fill / Spam：整个内部
    pub solid: bool,
    /// `show = line`
    pub show_line: bool,
    /// 自身音符是否作为边框（图案避开）
    pub own: bool,
    /// 带宽（keys）
    pub size_keys: f32,
    /// 每 key 多少 tick（`k × ppq`）
    pub tpk: f32,
    /// 工程 key 数
    pub nkeys: u16,
}

/// 返回 `(base 区域, 自身音符跨度)`；`base` 尚未做 invert/own 的最终加减
pub fn base_areas(input: &MaskAreaInput<'_>) -> (Area, Area) {
    let own = if input.own && !input.solid {
        Area::from_notes(input.notes)
    } else {
        Area::default()
    };
    let inside = input
        .polygon
        .map(|polygon| inside_area(polygon, input.nkeys))
        .unwrap_or_default();
    let line_only = input.show_line || input.polygon.is_none();
    let band = if input.solid {
        Area::default()
    } else if line_only || input.size_keys > 0.0 {
        band_area(input.notes, input.size_keys, input.tpk, input.nkeys)
    } else {
        Area::default()
    };
    let base = if input.solid {
        inside
    } else if line_only {
        band
    } else {
        inside.joined(&band)
    };
    (base, own)
}

/// 闭合轮廓的逐 key 行内部（扫描线；边界按 half-open，避免顶点重复）
pub fn inside_area(polygon: &[(f32, f32)], nkeys: u16) -> Area {
    if polygon.len() < 3 || nkeys == 0 {
        return Area::default();
    }
    let points: Vec<(f32, f32)> = polygon
        .iter()
        .copied()
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .collect();
    if points.len() < 3 {
        return Area::default();
    }
    let mut out = Vec::new();
    for key in 0..nkeys {
        let y = key as f32 + 0.5;
        let mut xs = Vec::new();
        for index in 0..points.len() {
            let (x1, y1) = points[index];
            let (x2, y2) = points[(index + 1) % points.len()];
            if (y1 <= y && y2 > y) || (y2 <= y && y1 > y) {
                let t = (y - y1) / (y2 - y1);
                xs.push(x1 + t * (x2 - x1));
            }
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        for pair in xs.as_chunks::<2>().0 {
            if pair[1] > pair[0] + EPS {
                out.push(RowSpan {
                    key,
                    start: pair[0],
                    end: pair[1],
                });
            }
        }
    }
    Area::new(out)
}

/// 圆刷带宽：每个 key 行上的音符段向两侧扩 `size/2` 个 key 的圆刷
///
/// `tpk` 是每 key 多少 tick（`k × ppq`），所以同一 `size_keys` 在不同 zoom 下
/// 换算出的 tick 宽度不同，但屏幕上的圆度不变。
pub fn band_area(notes: &[ShapeNote], size_keys: f32, tpk: f32, nkeys: u16) -> Area {
    let base = Area::from_notes(notes);
    if base.is_empty() || nkeys == 0 || !size_keys.is_finite() || size_keys < 0.0 {
        return Area::default();
    }
    let radius = size_keys / 2.0;
    if radius <= 0.0 || !tpk.is_finite() || tpk <= 0.0 {
        return base;
    }
    let steps = radius.floor() as i32;
    let mut out = Vec::new();
    for span in base.spans() {
        for dk in -steps..=steps {
            let reach = (tpk * (radius * radius - (dk * dk) as f32).max(0.0).sqrt()).round();
            let row = span.key as i32 + dk;
            if !(0..nkeys as i32).contains(&row) {
                continue;
            }
            out.push(RowSpan {
                key: row as u16,
                start: span.start - reach,
                end: span.end + reach,
            });
        }
    }
    Area::new(out)
}

#[cfg(test)]
mod tests;
