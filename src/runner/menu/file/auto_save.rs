//! 自动保存（两类，共用工程组包口径，互不影响）：
//!
//! 1. **兜底同步保存**（REND-019 PR-C 红线 9）：协作重连失败兜底态触发前，
//!    先同步落盘一次当前工程，随后才进入失败对话框。
//! 2. **周期自动保存**（默认关闭；设置 → 编辑 → 自动保存）：按用户设定间隔，
//!    把有未保存更改的工程异步写入**独立的**自动保存文件（配置目录
//!    `autosave/autosave.lmpj`）。异常退出（崩溃 / 强杀 / 断电）后文件留存，
//!    下次启动询问是否恢复；正常退出时删除。
//!
//! 最小侵入边界：周期自动保存与手动保存 / 保存确认（SaveConfirm）/ 关闭流程
//! **零耦合**——不触碰 `saving` 标志、不发进度事件、不修改用户的 .lmpj 源文件、
//! 不在各保存/加载/关闭路径挂清理钩子（清理只有「正常退出」一个时点）。

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use crate::runner::RunnerInner;
use crate::storage;

// 对话框文案仅在非测试构建使用；测试构建下 cfg 掉以免 unused import 告警
#[cfg(not(test))]
use lumino_core::{tr, trf};

/// 自动保存子目录（配置目录下；与 cloud_tmp / deleted_tracks 同一套布局约定）
const AUTOSAVE_DIR_NAME: &str = "autosave";
/// 自动保存文件名（单槽位：新一轮写入经原子替换整体覆盖旧内容）
const AUTOSAVE_FILE_NAME: &str = "autosave.lmpj";

// ── 周期自动保存：纯逻辑（与 winit / 存储解耦，可单测）────────────────────

/// 周期自动保存决策：同时满足「开关开启 + 工程有未保存更改 + 无进行中的
/// 写入任务」才执行写入。工程干净时跳过（避免空转写盘）。
pub(crate) fn autosave_due(enabled: bool, modified: bool, in_flight: bool) -> bool {
    enabled && modified && !in_flight
}

/// 下次触发时刻：`now + interval`（间隔经配置层收敛到 1..=120 分钟）
fn next_autosave_at(now: Instant, interval_mins: u32) -> Instant {
    now + Duration::from_secs(u64::from(interval_mins) * 60)
}

/// 自动保存文件路径：配置目录 `autosave/autosave.lmpj`
pub(crate) fn autosave_path() -> PathBuf {
    storage::config_dir()
        .join(AUTOSAVE_DIR_NAME)
        .join(AUTOSAVE_FILE_NAME)
}

/// 文件修改时刻的本地化展示（`%Y-%m-%d %H:%M`；取不到时返回占位文案）
#[cfg(not(test))]
fn modified_at_label(path: &std::path::Path) -> String {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map(chrono::DateTime::<chrono::Local>::from)
        .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|_| tr!("未知时间").to_string())
}

impl RunnerInner {
    // ── 周期自动保存 ──────────────────────────────────────────────────────

    /// 周期自动保存心跳：`about_to_wait` 每轮调用，未启用 / 未到点时廉价早退。
    ///
    /// 调度语义：
    /// - 开关关闭 → 清空排程（`control_flow` 随之不再为它唤醒）；
    /// - 到点 → 先重排下一轮（周期自本次时刻起算，不漂移），再按
    ///   [`autosave_due`] 决定是否真正写入。
    ///
    /// 与手动保存完全解耦：不提交进行中的 ghost 编辑——定时器不得打断用户
    /// 手势；半途编辑最多丢失到最后一次提交，属于可接受的自动保存粒度。
    pub(crate) fn autosave_tick(&mut self) {
        let (enabled, interval_mins) = {
            let ui = &self.window_state.storage.config.get().ui;
            (ui.autosave_enabled, ui.effective_autosave_interval_mins())
        };
        if !enabled {
            self.autosave_next = None;
            return;
        }
        let now = Instant::now();
        if self.autosave_next.is_some_and(|deadline| now < deadline) {
            return;
        }
        self.autosave_next = Some(next_autosave_at(now, interval_mins));

        let modified = self.window_state.window.ui().is_project_modified();
        let in_flight = self.autosave_in_flight.load(Ordering::SeqCst);
        if !autosave_due(true, modified, in_flight) {
            return;
        }
        self.run_periodic_autosave();
    }

    /// 执行一次周期自动保存：主线程组包（复用 `build_lmpj_project` 口径），
    /// tokio 阻塞任务经 DEBT-01 原子写协议落盘独立文件——写坏只影响自动
    /// 保存文件本身，用户的 .lmpj 源文件不受任何影响。
    ///
    /// `autosave_in_flight` 在组包前原子置位：组包是主线程同步段（大工程下
    /// 有成本），提前置位可防止上轮写入期与本轮组包重叠堆积。
    fn run_periodic_autosave(&mut self) {
        if self.autosave_in_flight.swap(true, Ordering::SeqCst) {
            return;
        }
        let path = autosave_path();
        if let Some(parent) = path.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            tracing::warn!("自动保存目录创建失败 {:?}: {e}", parent);
            self.autosave_in_flight.store(false, Ordering::SeqCst);
            return;
        }
        let Some(project) = self.build_lmpj_project(&path) else {
            // 无文档且无编辑器内容：无内容可存（不算错误，下个周期再试）
            self.autosave_in_flight.store(false, Ordering::SeqCst);
            return;
        };
        let in_flight = std::sync::Arc::clone(&self.autosave_in_flight);
        let path_for_log = path.clone();
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                lumino_export::save_to_archive(&project, &path)
            })
            .await;
            in_flight.store(false, Ordering::SeqCst);
            match result {
                Ok(Ok(())) => tracing::info!("周期自动保存完成: {:?}", path_for_log),
                Ok(Err(e)) => {
                    tracing::warn!("周期自动保存写入失败 {:?}: {e}", path_for_log)
                }
                Err(e) => {
                    tracing::warn!("周期自动保存任务失败 {:?}: {e}", path_for_log)
                }
            }
        });
    }

    /// 正常退出时清理自动保存文件：此时用户已对未保存更改做出明确处置
    /// （保存 / 放弃），自动保存文件失去意义；删除可避免下次启动误弹恢复
    /// 询问。崩溃 / 强杀不走此路径，文件留存供恢复。
    pub(crate) fn cleanup_autosave_on_exit(&self) {
        let path = autosave_path();
        if !path.exists() {
            return;
        }
        match std::fs::remove_file(&path) {
            Ok(()) => tracing::info!("正常退出，已清理自动保存文件: {:?}", path),
            Err(e) => tracing::warn!("自动保存文件清理失败 {:?}: {e}", path),
        }
    }

    /// 启动时自动保存恢复询问：文件存在即弹原生消息框（Yes/No 自定义按钮，
    /// 与 device_gate 的原生提示框同一套交互）。选「恢复」复用现有加载链路
    /// （`load_midi_file` 按 .lmpj 路由到工程加载事件流）；否则删除文件。
    ///
    /// 测试构建不弹框、不删文件、不自动加载（避免无头场景被模态框卡住）。
    pub(crate) fn prompt_autosave_recovery(&mut self) {
        let path = autosave_path();
        if !path.exists() {
            return;
        }
        #[cfg(not(test))]
        {
            let modified_at = modified_at_label(&path);
            let restore_label = tr!("恢复自动保存");
            let restore = matches!(
                rfd::MessageDialog::new()
                    .set_level(rfd::MessageLevel::Info)
                    .set_title(tr!("检测到未保存的自动保存"))
                    .set_description(trf!(
                        "发现上次会话的自动保存（修改于 {time}）。\n\n是否恢复其内容？恢复不会改动任何已有文件。",
                        time = modified_at
                    ))
                    .set_buttons(rfd::MessageButtons::OkCancelCustom(
                        restore_label.to_string(),
                        tr!("不恢复").to_string(),
                    ))
                    .show(),
                rfd::MessageDialogResult::Custom(label) if label == restore_label
            );
            if restore {
                tracing::info!("用户选择恢复自动保存: {:?}", path);
                // load_midi_file 自带 .lmpj / 素材 / MIDI 路由，直接复用
                self.load_midi_file(path);
            } else if let Err(e) = std::fs::remove_file(&path) {
                tracing::warn!("自动保存文件删除失败 {:?}: {e}", path);
            } else {
                tracing::info!("用户放弃恢复，已删除自动保存文件: {:?}", path);
            }
        }
        #[cfg(test)]
        {
            let _ = path;
        }
    }

    // ── 兜底同步保存（REND-019 PR-C，原有逻辑不动）────────────────────────

    /// Failed 兜底态触发前的**同步**自动保存（红线 9：必须先完成自动保存）。
    ///
    /// - 已有 .lmpj 源 → 覆盖保存原文件；
    /// - 否则写恢复文件 `lumino-recovery-<unix秒>.lmpj` 到文档目录（无则临时目录）。
    ///
    /// 与常规保存共用工程构建口径（`save::build_lmpj_project`），但不经 tokio——
    /// 返回时工程已落盘，随后才进入失败兜底对话框。
    pub(crate) fn auto_save_before_failed(&mut self) -> Option<PathBuf> {
        let path = self
            .midi_state
            .current_midi_source
            .clone()
            .filter(|p| super::helpers::get_file_extension(p) == "lmpj")
            .unwrap_or_else(recovery_path);

        match self.build_lmpj_project(&path) {
            Some(project) => match lumino_export::save_to_archive(&project, &path) {
                Ok(()) => {
                    tracing::info!("重连失败兜底：工程已自动保存到 {:?}", path);
                    Some(path)
                }
                Err(e) => {
                    tracing::error!("重连失败兜底：自动保存写入失败 {:?}: {e}", path);
                    None
                }
            },
            None => {
                tracing::warn!("重连失败兜底：没有可保存的工程内容");
                None
            }
        }
    }
}

/// 恢复文件路径：文档目录（无则临时目录）`lumino-recovery-<unix秒>.lmpj`。
fn recovery_path() -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let name = format!("lumino-recovery-{stamp}.lmpj");
    directories::UserDirs::new()
        .and_then(|d| d.document_dir().map(|dir| dir.join(&name)))
        .unwrap_or_else(|| std::env::temp_dir().join(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 决策表：开关关闭一律不写，即使工程是脏的
    #[test]
    fn test_autosave_due_disabled_never_writes() {
        assert!(!autosave_due(false, true, false));
        assert!(!autosave_due(false, true, true));
        assert!(!autosave_due(false, false, false));
    }

    /// 决策表：开关开启但工程干净（刚保存过 / 刚加载）不写
    #[test]
    fn test_autosave_due_clean_project_skips() {
        assert!(!autosave_due(true, false, false));
    }

    /// 决策表：开关开启、工程脏、无进行中任务 → 写
    #[test]
    fn test_autosave_due_dirty_project_writes() {
        assert!(autosave_due(true, true, false));
    }

    /// 决策表：上一轮写入尚未完成时本轮跳过（防任务堆积）
    #[test]
    fn test_autosave_due_in_flight_skips() {
        assert!(!autosave_due(true, true, true));
    }

    /// 排程：下一次触发点 = 当前时刻 + 间隔分钟
    #[test]
    fn test_next_autosave_at_interval() {
        let now = Instant::now();
        let next = next_autosave_at(now, 5);
        let elapsed = next.duration_since(now);
        assert_eq!(
            elapsed,
            Duration::from_secs(5 * 60),
            "5 分钟间隔应换算为 300 秒"
        );
    }
}
