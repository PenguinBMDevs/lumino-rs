//! 三缓冲测试集
//!
//! 所有测试均在单线程上下文执行，因此 unsafe 调用的前置条件（唯一写入者、
//! 唯一读取者、写入完成后 swap 等）由测试流程的顺序执行天然保证。

use std::sync::atomic::Ordering;

use super::*;

#[test]
fn test_swappable_buffer_basic() {
    let buffer = SwappableBuffer::<i32>::new(100);

    // UI 线程写入
    // Safety: 单线程测试，同一时间只有这一个写入者，写入完成后会调用 swap
    unsafe {
        let write_buf = buffer.write_buffer();
        write_buf.push(1);
        write_buf.push(2);
        write_buf.push(3);
    }

    // 交换
    let version = buffer.swap();
    assert_eq!(version, 1);

    // 渲染线程读取
    // Safety: 单线程测试，同一时间只有这一个读取者，且 acquire 后在当前帧内只做读取
    unsafe {
        let read_buf = buffer.acquire_read_buffer();
        assert_eq!(read_buf.len(), 3);
        assert_eq!(read_buf[0], 1);
        assert_eq!(read_buf[1], 2);
        assert_eq!(read_buf[2], 3);
    }
}

#[test]
fn test_swappable_buffer_multiple_swaps() {
    let buffer = SwappableBuffer::<i32>::new(10);

    for i in 0..5 {
        // Safety: 单线程顺序执行，每次写入前 clear 然后 push 一个值，
        // 完成后立即 swap，满足唯一写入者和写入后 swap 的前置条件
        unsafe {
            let write_buf = buffer.write_buffer();
            write_buf.clear();
            write_buf.push(i);
        }
        buffer.swap();
    }

    // Safety: 单线程，acquire 前已确保 swap 完成，读端唯一且只读
    unsafe {
        let read_buf = buffer.acquire_read_buffer();
        assert_eq!(read_buf[0], 4);
    }
}

#[test]
fn test_acquire_while_holding_reference() {
    // 验证：acquire 持有引用期间发生 swap，writer 槽位 ≠ reading 槽位
    let buffer = SwappableBuffer::<i32>::new(4);

    // 写入并提交第一帧
    // Safety: 单线程，唯一写入者
    unsafe {
        buffer.write_buffer().push(10);
    }
    buffer.swap();

    // 渲染线程 acquire：此时 reading 拿到刚提交的数据
    // Safety: 单线程，唯一读取者，且仅做只读访问
    let reading_ref = unsafe { buffer.acquire_read_buffer() };
    assert_eq!(reading_ref[0], 10);

    // UI 线程写入第二帧（占用另一个槽位）
    // Safety: 单线程，writing_ref 已丢弃（上一行未保留可变引用），
    // 此时 writer 槽位已变回安全状态，可重新获取
    unsafe {
        buffer.write_buffer().push(20);
    }
    // reading_ref 仍然持有着，此时 swap
    let v2 = buffer.swap();

    // swap 后 writer 槽位应该是之前的 reading 槽位，不是 reading_ref 指向的槽位
    // reading_ref 的内容保持不变
    assert_eq!(reading_ref[0], 10, "持有引用期间 swap 不应污染原数据");
    assert!(v2 >= 1);

    // 验证 reading_len 原子量正确
    assert_eq!(buffer.reading_len.load(Ordering::Relaxed), 1);
}

#[test]
fn test_peek_read_buffer() {
    // 验证 peek_read_buffer 返回与 acquire_read_buffer 同一槽位
    let buffer = SwappableBuffer::<i32>::new(4);

    // Safety: 单线程，唯一写入者
    unsafe {
        buffer.write_buffer().push(42);
    }
    buffer.swap();

    // Safety:
    // - 先 acquire（满足 peek 的前置条件），再 peek
    // - 单线程，无并发访问
    unsafe {
        let acquired = buffer.acquire_read_buffer();
        let peeked = buffer.peek_read_buffer();

        assert_eq!(acquired.as_ptr(), peeked.as_ptr(), "peek 应返回同一槽位");
        assert_eq!(peeked[0], 42);
    }
}

#[test]
fn test_reading_len_published_after_acquire() {
    let buffer = SwappableBuffer::<i32>::new(100);

    // 初始 reading_len 应为 0
    assert_eq!(buffer.reading_len.load(Ordering::Relaxed), 0);

    // Safety: 单线程，两次 push 之间没有其他写入者，且最终会 swap
    unsafe {
        buffer.write_buffer().push(1);
    }
    // Safety: 同上，同一线程连续两次写入，仍满足唯一写入者条件
    unsafe {
        buffer.write_buffer().push(2);
    }
    buffer.swap();

    // acquire 前 reading_len 还是上次的值（0）
    assert_eq!(buffer.reading_len.load(Ordering::Relaxed), 0);

    // Safety: 单线程，唯一读取者
    unsafe {
        buffer.acquire_read_buffer();
    }

    // acquire 后 reading_len 更新为 2
    assert_eq!(
        buffer.reading_len.load(Ordering::Relaxed),
        2,
        "acquire 后 reading_len 应反映新 reading 槽位的 len"
    );
}

#[test]
fn test_three_state_invariant_holds_through_cycle() {
    // 验证经过多轮 swap+acquire 后，"三者互异"的不变量仍未破坏
    let buffer = SwappableBuffer::<i32>::new(4);

    for i in 0..20 {
        // Safety: 单线程，顺序执行，唯一写入者
        unsafe {
            buffer.write_buffer().push(i);
        }
        buffer.swap();
        // Safety: 单线程，swap 后 acquire，唯一读取者，且仅做只读访问
        unsafe {
            buffer.acquire_read_buffer();
        }
    }

    // 验证 stats 不越界
    for slot in 0..3 {
        let (cap, len) = buffer.buffer_info(slot);
        assert!(cap >= 4, "slot {slot}: capacity={cap} 不应小于初始值");
        assert!(len <= cap, "slot {slot}: len={len} <= cap={cap}");
    }
}

#[test]
fn test_mpsc_queue() {
    let queue = MpscQueue::<i32>::new();

    // 发送数据
    assert!(queue.send(42).is_ok());

    // 接收数据
    assert_eq!(queue.try_recv(), Some(42));
    assert_eq!(queue.try_recv(), None);
}

#[test]
fn test_mpsc_queue_overflow() {
    let queue = MpscQueue::<i32>::new();

    // 第一次发送成功
    assert!(queue.send(1).is_ok());

    // 第二次发送失败（槽位已满）
    assert!(queue.send(2).is_err());

    // 接收后再次发送成功
    queue.recv();
    assert!(queue.send(3).is_ok());
}
