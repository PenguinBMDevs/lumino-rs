//! 文档切换时的合成管线布局过渡决策（REND-002 / REND-010 #102 N-5）。
//!
//! XSynth（CPU）与 LGS（GPU）两条软件后端共用同一语义：文档切换时若端口布局
//! 未变，只做轻量复位（微秒级，不重开音频流）；布局变化才全量重建
//! （100ms+，重开音频流 / join 通道线程）。
//!
//! 为什么抽成纯函数：真实重建依赖 cpal 硬件路径，无法在单测中构造后端实例；
//! 本模块锁定“同布局绝不重建”的 N-2 性能契约与“异布局必须重建”的正确性契约，
//! 两个后端只允许通过 [`layout_action`] 决策，避免两处各写分支后单边退化。

/// 布局过渡动作（文档切换时合成管线该做什么）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LayoutAction {
    /// 布局未变：轻量复位通道状态（不重开音频流）。
    LightReset,
    /// 布局变化：全量重建合成管线。
    Rebuild,
}

/// 依据当前布局与目标布局决策过渡动作（纯函数，可单测）。
pub(crate) fn layout_action(current: u8, target: u8) -> LayoutAction {
    if current == target {
        LayoutAction::LightReset
    } else {
        LayoutAction::Rebuild
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// N-2 性能契约：同布局绝不全量重建（含 0、边界与超上限值）。
    #[test]
    fn same_layout_never_rebuilds() {
        for port in [0u8, 1, 6, 15, 16, 127, 255] {
            assert_eq!(
                layout_action(port, port),
                LayoutAction::LightReset,
                "同布局（max_port={port}）必须走轻量复位"
            );
        }
    }

    /// 正确性契约：布局变化（扩展/缩回/跳变）必须全量重建。
    #[test]
    fn changed_layout_always_rebuilds() {
        for (current, target) in [(0u8, 1u8), (1, 0), (6, 2), (2, 6), (0, 127)] {
            assert_eq!(
                layout_action(current, target),
                LayoutAction::Rebuild,
                "{current} -> {target} 必须全量重建"
            );
        }
    }
}
