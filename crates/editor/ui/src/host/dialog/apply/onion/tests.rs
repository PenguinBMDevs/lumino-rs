//! 洋葱皮后端切换裁决测试（纯函数，不初始化 `Host`/GPU）

use super::{SwitchContext, SwitchPlan, plan_switch};
use lumino_core::storage::config::OnionSkinBackend;

/// 基准上下文：解锁、横向卷帘、无模式冲突、无导出。
fn base(requested: OnionSkinBackend, old: OnionSkinBackend) -> SwitchContext {
    SwitchContext {
        old,
        requested,
        locked: false,
        vertical_roll: false,
        arrangement: false,
        waterfall: false,
        exporting: false,
    }
}

#[test]
fn test_same_backend_is_keep() {
    assert_eq!(
        plan_switch(base(OnionSkinBackend::Gpu, OnionSkinBackend::Gpu)),
        SwitchPlan::Keep
    );
    assert_eq!(
        plan_switch(base(OnionSkinBackend::Cpu, OnionSkinBackend::Cpu)),
        SwitchPlan::Keep,
        "值与运行时相同：不切换、不弹提示（重复保存不产生噪音）"
    );
}

#[test]
fn test_unlocked_switch_is_applied() {
    assert_eq!(
        plan_switch(base(OnionSkinBackend::Cpu, OnionSkinBackend::Gpu)),
        SwitchPlan::Switch
    );
    assert_eq!(
        plan_switch(base(OnionSkinBackend::Gpu, OnionSkinBackend::Cpu)),
        SwitchPlan::Switch,
        "切回 GPU 永远安全：不受纵向卷帘/走带/瀑布流/导出限制"
    );
}

#[test]
fn test_locked_rejects_both_directions() {
    let ctx = SwitchContext {
        locked: true,
        ..base(OnionSkinBackend::Cpu, OnionSkinBackend::Gpu)
    };
    assert_eq!(
        plan_switch(ctx),
        SwitchPlan::RejectLocked,
        "加载 MIDI 后禁止切到 CPU"
    );

    let ctx = SwitchContext {
        locked: true,
        ..base(OnionSkinBackend::Gpu, OnionSkinBackend::Cpu)
    };
    assert_eq!(
        plan_switch(ctx),
        SwitchPlan::RejectLocked,
        "锁定是「禁止切换后端」，不是「只禁止切到 CPU」"
    );
}

#[test]
fn test_vertical_roll_forces_horizontal_before_switch() {
    let ctx = SwitchContext {
        vertical_roll: true,
        ..base(OnionSkinBackend::Cpu, OnionSkinBackend::Gpu)
    };
    assert_eq!(plan_switch(ctx), SwitchPlan::ForceHorizontalThenSwitch);
}

#[test]
fn test_incompatible_modes_block_cpu() {
    let arrangement = SwitchContext {
        arrangement: true,
        ..base(OnionSkinBackend::Cpu, OnionSkinBackend::Gpu)
    };
    assert_eq!(plan_switch(arrangement), SwitchPlan::RejectIncompatibleMode);

    let waterfall = SwitchContext {
        waterfall: true,
        ..base(OnionSkinBackend::Cpu, OnionSkinBackend::Gpu)
    };
    assert_eq!(plan_switch(waterfall), SwitchPlan::RejectIncompatibleMode);

    let exporting = SwitchContext {
        exporting: true,
        ..base(OnionSkinBackend::Cpu, OnionSkinBackend::Gpu)
    };
    assert_eq!(plan_switch(exporting), SwitchPlan::RejectIncompatibleMode);
}

/// 不兼容模式优先于纵向卷帘：自动横滚是**副作用**，不能在一个注定被拒的
/// 请求上先把用户的视图改掉。
#[test]
fn test_incompatible_mode_beats_vertical_roll() {
    let ctx = SwitchContext {
        vertical_roll: true,
        waterfall: true,
        ..base(OnionSkinBackend::Cpu, OnionSkinBackend::Gpu)
    };
    assert_eq!(plan_switch(ctx), SwitchPlan::RejectIncompatibleMode);
}

/// 纵向卷帘只拦「切到 CPU」；切回 GPU 不该被视图状态拦。
#[test]
fn test_vertical_roll_allows_switch_back_to_gpu() {
    let ctx = SwitchContext {
        vertical_roll: true,
        arrangement: true,
        ..base(OnionSkinBackend::Gpu, OnionSkinBackend::Cpu)
    };
    assert_eq!(plan_switch(ctx), SwitchPlan::Switch);
}
