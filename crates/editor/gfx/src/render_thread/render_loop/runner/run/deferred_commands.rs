use super::*;

use super::super::deferred::handle_deferred_command;

/// 处理延迟队列中的控制命令（视频导出 / 贴图瀑布流控制）。
pub(super) fn process_deferred_commands(
    context: &mut DeferredCommandContext<'_>,
    deferred: &mut Vec<ControlCommand>,
) {
    for cmd in deferred.drain(..) {
        handle_deferred_command(cmd, context);
    }
}
