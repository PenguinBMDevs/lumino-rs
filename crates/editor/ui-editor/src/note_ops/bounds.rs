//! 选中包围盒累加器。
//!
//! `selected_bounds` 缓存是 `(min_tick, max_end_tick, max_key, min_key)`。
//! 此前 6 处扫描各自维护四个局部变量并手写 min/max（drag/selection 两处、
//! select_all_notes、selection_assign、selection_box 两条回退路径），
//! 口径容易漂移（长度还是终点、key 类型转换）。统一走本累加器。

/// 选中音符的包围盒累加器（无音符时 `get()` 为 `None`）。
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct Bounds(Option<(f32, f32, u16, u16)>);

impl Bounds {
    /// 累加一个音符：`start` / `end` 为 tick 区间（`end` = 起点 + 长度）。
    pub(crate) fn add(&mut self, start: f32, end: f32, key: u16) {
        self.0 = Some(match self.0 {
            Some((min_t, max_te, max_k, min_k)) => (
                min_t.min(start),
                max_te.max(end),
                max_k.max(key),
                min_k.min(key),
            ),
            None => (start, end, key, key),
        });
    }

    /// 取出缓存元组。
    pub(crate) fn get(self) -> Option<(f32, f32, u16, u16)> {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_bounds_is_none() {
        assert_eq!(Bounds::default().get(), None);
    }

    #[test]
    fn add_tracks_min_start_max_end_and_key_span() {
        let mut b = Bounds::default();
        b.add(10.0, 20.0, 5);
        b.add(4.0, 8.0, 9);
        b.add(6.0, 30.0, 2);
        assert_eq!(b.get(), Some((4.0, 30.0, 9, 2)));
    }
}
