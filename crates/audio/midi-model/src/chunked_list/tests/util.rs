//! ChunkedList 单元测试共享工具类型与辅助函数

use std::sync::Arc;

use crate::chunked_list::{ChunkedList, EventTick};

/// 测试用最小事件类型
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TestEvent {
    pub(super) tick: u32,
    pub(super) id: u32,
}

impl EventTick for TestEvent {
    fn tick(&self) -> u32 {
        self.tick
    }
}

pub(super) fn make_test_event(tick: u32, id: u32) -> TestEvent {
    TestEvent { tick, id }
}

pub(super) fn sorted_events(count: usize) -> Vec<TestEvent> {
    (0..count as u32)
        .map(|i| make_test_event(i * 10, i))
        .collect()
}

/// 直接构造多块 ChunkedList（测试窗口跨块迭代，避免依赖 50 万真实容量）
pub(super) fn multi_chunk(list_sizes: &[usize]) -> ChunkedList<TestEvent> {
    let mut list: ChunkedList<TestEvent> = ChunkedList::new();
    let mut next_tick = 0u32;
    for &size in list_sizes {
        let chunk: Vec<TestEvent> = (0..size)
            .map(|i| make_test_event(next_tick + i as u32, next_tick + i as u32))
            .collect();
        list.chunks.push(Arc::new(chunk));
        next_tick += size as u32;
    }
    list.total_len = next_tick as usize;
    list.rebuild_index();
    list
}

/// 参照实现：普通 Vec + partition_point（验证 ChunkedList 行为等价）
pub(super) fn reference_insert(sorted: &mut Vec<TestEvent>, e: TestEvent) {
    let idx = sorted.partition_point(|x| x.tick <= e.tick);
    sorted.insert(idx, e);
}
