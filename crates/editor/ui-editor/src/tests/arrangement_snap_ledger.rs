//! 走带（arrangement）框选量化口径**台账固化**测试。
//!
//! 背景（本卡决策项）：卷帘框选 X 向采用「单元覆盖式」量化（小端取单元低边、
//! 大端取单元高边），而走带框选使用 `arrangement::interaction::geometry::snap_tick`
//! —— **round + 拍号感知**（段内按拍长对齐）。两者语义**刻意不同**：
//!
//! - 卷帘要「选框覆盖鼠标触碰过的单元」，边界必须**外扩到单元格外沿**；
//! - 走带要「框选/移动结果与按拍号绘制的网格线一致」，边界取**最近格线**。
//!
//! 本卡不把走带纳入统一（`geometry::snap_tick` 有多个共享调用方，改动前需先梳理
//! 调用链）。这里用测试把现状**钉住**：一旦有人把走带的 round 改成 floor/单元覆盖，
//! 或破坏拍号感知，本文件立刻变红，提醒先走决策流程而不是顺手改。

use crate::arrangement::interaction::geometry;
use lumino_core::NotePrecision;

/// 无拍号：回退固定 `ppq` 间隔 + round（1900 → 1920，而非 floor 到 1920 之外的 0）
#[test]
fn test_arrangement_snap_tick_rounds_without_time_signatures() {
    assert_eq!(
        geometry::snap_tick(1900.0, NotePrecision::Quarter, 1920, &[]),
        1920.0,
        "1/4 精度 @1920：1900 应 round 到最近格线 1920"
    );
    assert_eq!(
        geometry::snap_tick(1000.0, NotePrecision::Quarter, 1920, &[]),
        1920.0,
        "1/4 精度 @1920：1000 应 round 到 1920（floor 会得到 0）"
    );
    assert_eq!(
        geometry::snap_tick(900.0, NotePrecision::Quarter, 1920, &[]),
        0.0,
        "1/4 精度 @1920：900 应 round 回 0"
    );
}

/// 有拍号（4/4）：段内按拍长对齐，round 语义与上面一致
#[test]
fn test_arrangement_snap_tick_keeps_time_signature_awareness() {
    let ts = [(0u32, 4u8, 4u8)];
    assert_eq!(
        geometry::snap_tick(1900.0, NotePrecision::Quarter, 1920, &ts),
        1920.0,
        "4/4 段内 1/4 精度间隔 = 1 拍 = 1920"
    );
    assert_eq!(
        geometry::snap_tick(1900.0, NotePrecision::Eighth, 1920, &ts),
        1920.0,
        "4/4 段内 1/8 精度间隔 = 1/2 拍 = 960：1900 round 到 1920"
    );
    // 3/4 段：倍拍长，网格线随之变化（拍号感知未被绕过）
    let ts34 = [(0u32, 3u8, 4u8)];
    assert_eq!(
        geometry::snap_tick(1900.0, NotePrecision::Quarter, 1920, &ts34),
        1920.0,
        "3/4 段内 1/4 精度仍为 1 拍：1900 round 到 1920"
    );
}
