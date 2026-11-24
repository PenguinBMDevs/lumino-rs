//! 动作通道 helper：单值先到先得槽（Slot）与多值队列（Queue）。
//!
//! 两种 helper 都通过 `egui::Context` 的临时数据传递，语义与原先 38 份
//! 手写实现一致：单值槽先到先得（非破坏性检查，绝不吞已排队动作），
//! 多值队列按发生顺序累积，取走即清空。

use std::marker::PhantomData;

use egui::{Context, Id};

/// 单值动作槽：先到先得，取走即清空。
pub struct Slot<T> {
    id: Id,
    _marker: PhantomData<fn() -> T>,
}

impl<T: Clone + Send + Sync + 'static> Slot<T> {
    pub fn new(id: Id) -> Self {
        Self {
            id,
            _marker: PhantomData,
        }
    }

    /// 覆盖写入：后到的动作覆盖先到的（无先到先得检查）。
    pub fn push(&self, ctx: &Context, action: T) {
        ctx.data_mut(|d| d.insert_temp(self.id, Some(action)));
    }

    /// 先到先得：已有动作时丢弃新动作。
    ///
    /// 必须用 `get_temp` 做非破坏性检查——`take().is_none()` 会把已排队
    /// 的动作取走丢弃，违反「先到先得」语义。
    pub fn push_first(&self, ctx: &Context, action: T) {
        if ctx
            .data(|d| d.get_temp::<Option<T>>(self.id))
            .flatten()
            .is_some()
        {
            return;
        }
        ctx.data_mut(|d| d.insert_temp(self.id, Some(action)));
    }

    /// 取走动作。
    pub fn take(&self, ctx: &Context) -> Option<T> {
        ctx.data_mut(|d| d.remove_temp::<Option<T>>(self.id))
            .flatten()
    }
}

/// 多值动作队列：按发生顺序累积，取走即清空。
pub struct Queue<T> {
    id: Id,
    _marker: PhantomData<fn() -> T>,
}

impl<T: Clone + Send + Sync + 'static> Queue<T> {
    pub fn new(id: Id) -> Self {
        Self {
            id,
            _marker: PhantomData,
        }
    }

    /// 追加动作。
    pub fn push(&self, ctx: &Context, action: T) {
        let mut actions = ctx
            .data(|d| d.get_temp::<Vec<T>>(self.id))
            .unwrap_or_default();
        actions.push(action);
        ctx.data_mut(|d| d.insert_temp(self.id, actions));
    }

    /// 取走全部动作（按发生顺序）。
    pub fn take(&self, ctx: &Context) -> Vec<T> {
        ctx.data_mut(|d| d.remove_temp::<Vec<T>>(self.id))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 先到先得必须非破坏性：第二次 push 不得吞掉第一次已排队的动作。
    #[test]
    fn slot_first_wins_is_non_destructive() {
        let ctx = Context::default();
        let slot: Slot<u32> = Slot::new(Id::unique("test_slot_first_wins"));
        slot.push_first(&ctx, 1);
        slot.push_first(&ctx, 2);
        assert_eq!(slot.take(&ctx), Some(1));
        assert_eq!(slot.take(&ctx), None);
    }

    #[test]
    fn queue_preserves_order() {
        let ctx = Context::default();
        let queue: Queue<u32> = Queue::new(Id::unique("test_queue_order"));
        queue.push(&ctx, 1);
        queue.push(&ctx, 2);
        assert_eq!(queue.take(&ctx), vec![1, 2]);
        assert!(queue.take(&ctx).is_empty());
    }
}
