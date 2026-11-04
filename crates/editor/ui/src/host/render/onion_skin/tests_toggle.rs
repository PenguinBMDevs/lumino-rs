//! 洋葱皮全局开关（工具栏按钮）→ 决策层契约。
//!
//! 用户需求：默认关闭、点击后显示全部音轨的洋葱皮、关闭就彻底关闭；
//! GPU 模式下关闭**不卸载数据**，只是不显示。
//!
//! 因此开关变化只能产出 `ViewState`（一条 uniform 消息），
//! 绝不能产出 `Full`（全量重传）或 `Delta`（段重建）——后者会把「关一下再打开」
//! 变成一次黑乐谱级别的全量实例重建。
//!
//! 独立文件以保持 `onion_skin/tests.rs` < 400 行；构造器复用 `tests` 模块。

use super::tests::{assert_none, make_fp};
use super::*;

/// 断言：只发 `ViewState`（uniform 零重传），不得触发任何数据重建。
fn assert_view_state_only(action: &OnionSkinAction) {
    match action {
        OnionSkinAction::ViewState => {}
        OnionSkinAction::Full => panic!("开关变化不得触发全量重传（Full）：{action:?}"),
        OnionSkinAction::Delta(tracks) => {
            panic!("开关变化不得触发段重建（Delta {tracks:?}）：{action:?}")
        }
        OnionSkinAction::None => panic!("开关变化必须同步 uniform，实际 None"),
    }
}

/// 带指定开关位的指纹（其余字段取默认构造）
fn fp_with_visibility(onion_visible: bool) -> OnionSkinFingerprint {
    let mut fp = make_fp(42, 0, 3, 0);
    fp.onion_visible = onion_visible;
    fp
}

/// 打开洋葱皮：只发 `ViewState`，数据零重传。
#[test]
fn onion_toggle_on_emits_view_state_only() {
    let mut state = OnionSkinState::default();
    state.mark_built(&fp_with_visibility(false));

    assert_view_state_only(&state.decide_action(&fp_with_visibility(true)));
}

/// 关闭洋葱皮：同样只发 `ViewState`（数据留在显存，重新打开即时生效）。
#[test]
fn onion_toggle_off_emits_view_state_only() {
    let mut state = OnionSkinState::default();
    state.mark_built(&fp_with_visibility(true));

    assert_view_state_only(&state.decide_action(&fp_with_visibility(false)));
}

/// 开关值未变 → `None`（每帧调用，不得刷 uniform 消息）。
#[test]
fn onion_toggle_unchanged_is_none() {
    let mut state = OnionSkinState::default();
    state.mark_built(&fp_with_visibility(true));
    assert_none(&state.decide_action(&fp_with_visibility(true)));

    // 关闭态同理
    state.mark_built(&fp_with_visibility(false));
    assert_none(&state.decide_action(&fp_with_visibility(false)));
}

/// 切换后 `mark_built` 必须记录新值：同一指纹重复驱动不得重复发消息。
#[test]
fn onion_toggle_value_is_recorded_after_sync() {
    let mut state = OnionSkinState::default();
    state.mark_built(&fp_with_visibility(false));

    let fp = fp_with_visibility(true);
    assert_view_state_only(&state.decide_action(&fp));
    state.mark_built(&fp);
    assert_none(&state.decide_action(&fp));
}

/// CPU 单轨模式同样要同步开关位：否则切回 GPU 后端时 uniform 与实际状态错位
///（显存里没有洋葱皮实例可隐藏，但状态机必须一致）。
#[test]
fn onion_toggle_in_cpu_mode_emits_view_state() {
    let mut state = OnionSkinState::default();
    state.set_cpu_current_only(true);
    state.mark_built(&fp_with_visibility(false));

    assert_view_state_only(&state.decide_action(&fp_with_visibility(true)));
}
