//! 互斥浮层的唯一存储与操作。
//!
//! 旧实现用 6 个 `bool`，靠 `dismiss_popovers` 在每次事件前逐个纠正；漏一处就会
//! 出现两个下拉同时打开。现在存储只有一个 `Option<Popover>`：打开即顶替，
//! 「至多一个」由类型保证。

use super::*;

impl Toolbar {
    /// 当前打开的互斥浮层（`None` = 全部收起）。
    #[must_use]
    pub fn active_popover(&self) -> Option<Popover> {
        self.active_popover
    }

    /// 指定浮层是否打开。
    #[must_use]
    pub fn is_popover_open(&self, popover: Popover) -> bool {
        self.active_popover == Some(popover)
    }

    /// 打开指定浮层；其余浮层自动关闭（单存储即互斥）。
    pub fn open_popover(&mut self, popover: Popover) {
        self.active_popover = Some(popover);
    }

    /// 关闭指定浮层；只关它自己，别的浮层不受影响。
    pub fn close_popover(&mut self, popover: Popover) {
        if self.active_popover == Some(popover) {
            self.active_popover = None;
        }
    }

    /// 开合指定浮层；与 `close` 后再 `open` 等价。
    pub fn toggle_popover(&mut self, popover: Popover) {
        if self.active_popover == Some(popover) {
            self.active_popover = None;
        } else {
            self.active_popover = Some(popover);
        }
    }

    /// 收起全部互斥浮层。
    pub fn close_all_popovers(&mut self) {
        self.active_popover = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 单存储的核心契约：打开 B 必然关掉 A；关错对象不误伤当前浮层。
    #[test]
    fn opening_a_popover_closes_the_previous_one() {
        let mut toolbar = Toolbar::new();
        toolbar.open_popover(Popover::BrushDropdown);
        assert!(toolbar.is_popover_open(Popover::BrushDropdown));

        toolbar.open_popover(Popover::ShapeDropdown);
        assert_eq!(toolbar.active_popover(), Some(Popover::ShapeDropdown));
        assert!(!toolbar.is_popover_open(Popover::BrushDropdown));

        // 关闭一个没打开的浮层，不得影响当前打开的那个。
        toolbar.close_popover(Popover::BrushDropdown);
        assert!(toolbar.is_popover_open(Popover::ShapeDropdown));

        toolbar.toggle_popover(Popover::ShapeDropdown);
        assert_eq!(toolbar.active_popover(), None);

        toolbar.open_popover(Popover::TextParams);
        toolbar.close_all_popovers();
        assert_eq!(toolbar.active_popover(), None);
    }
}
