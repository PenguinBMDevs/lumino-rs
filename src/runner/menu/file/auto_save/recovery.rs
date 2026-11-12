//! 自动保存恢复状态机：启动询问、损坏保命、备份回退。
//!
//! 恢复必须作为“未保存工程”载入：
//! - 清空 `current_midi_source` / 云端来源；
//! - 标记 `modified = true`；
//! - 不覆盖 autosave，也不把 autosave 当作可 Ctrl+S 覆盖的源文件。
//!
//! DEBT-12：分支判定全部下沉到 [`plan`]（纯函数，可脱离 winit 单测）；
//! 启动询问改为应用内对话框 `DialogType::AutosaveRecover`，原生 `MessageDialog`
//! 仅保留为平台回退（Android / 测试模式）。

mod plan;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

pub(crate) use plan::may_rotate_backup;

use lumino_core::{tr, trf};
use lumino_platform::dialog::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
use lumino_ui::message::AutosaveRecoverAction;

use crate::runner::RunnerInner;

use super::{autosave_backup_path, autosave_path};
use plan::{LoadFailureNext, RecoverySource};

/// 一次自动保存恢复的候选状态。
#[derive(Debug, Clone)]
pub(crate) struct AutosaveRecovery {
    /// 主自动保存路径（失败时改名保留）。
    pub main: PathBuf,
    /// 备份路径（主文件损坏/缺失时的回退候选）。
    pub backup: PathBuf,
    /// 当前加载的是备份而不是主文件。
    pub using_backup: bool,
}

impl RunnerInner {
    /// 启动时询问是否恢复上次会话留下的自动保存。
    ///
    /// 主文件存在时优先主文件；主文件缺失但备份存在时直接以备份为候选。
    /// 用户选择不恢复时把文件改名为 `*-declined-*` 保留，下次不再询问。
    ///
    /// DEBT-12：桌面平台走应用内对话框（非阻塞）；Android / 测试模式保留
    /// 原生模态框作为回退，语义完全一致。
    pub(crate) fn prompt_autosave_recovery(&mut self) {
        let main = autosave_path();
        let backup = autosave_backup_path();
        let Some(source) = plan::startup_candidate(main.exists(), backup.exists()) else {
            return;
        };
        let candidate = source.path(&main, &backup);
        let modified_at = modified_at_label(&candidate);

        if self.use_native_recovery_prompt() {
            let restore = ask_recovery_native(&candidate, &modified_at);
            self.begin_autosave_recovery(restore, source);
            return;
        }

        // 应用内对话框：只排队建窗，用户选择后由
        // `resolve_autosave_prompt` 继续（不阻塞事件循环）。
        self.autosave_prompt_pending = true;
        self.window_state
            .dialog_manager
            .open_autosave_recover(candidate.display().to_string(), modified_at);
    }

    /// 应用内恢复对话框的结果（Runner 结果分派入口）。
    pub(crate) fn resolve_autosave_prompt(&mut self, action: AutosaveRecoverAction) {
        if !self.autosave_prompt_pending {
            tracing::debug!("自动保存恢复对话框结果到达，但当前没有待处理的询问");
            return;
        }
        self.autosave_prompt_pending = false;

        let main = autosave_path();
        let backup = autosave_backup_path();
        let Some(source) = plan::startup_candidate(main.exists(), backup.exists()) else {
            return;
        };
        self.begin_autosave_recovery(action == AutosaveRecoverAction::Restore, source);
    }

    /// 恢复对话框被窗口 X 关闭：不做任何决定，也不改名。
    ///
    /// 与「不恢复」区分：误关窗口不应导致自动保存被改名（那会让用户下次
    /// 启动再也看不到候选）。文件保留，下次启动继续询问。
    pub(crate) fn dismiss_autosave_prompt(&mut self) {
        if self.autosave_prompt_pending {
            tracing::info!("自动保存恢复对话框被关闭，保留文件待下次启动询问");
        }
        self.autosave_prompt_pending = false;
        // 未做决定 = 文件仍是用户唯一副本：正常退出不得清理。
        self.autosave_preserve_on_exit = true;
    }

    /// 执行用户选择：恢复或放弃。
    fn begin_autosave_recovery(&mut self, restore: bool, source: RecoverySource) {
        let main = autosave_path();
        let backup = autosave_backup_path();
        if restore {
            let candidate = source.path(&main, &backup);
            tracing::info!("用户选择恢复自动保存: {:?}", candidate);
            self.autosave_recovery = Some(AutosaveRecovery {
                main,
                backup,
                using_backup: source.is_backup(),
            });
            self.load_midi_file(candidate);
        } else {
            tracing::info!("用户放弃恢复自动保存，改名保留");
            rename_to_unique(&main, plan::DECLINED_TAG);
            rename_to_unique(&backup, plan::DECLINED_TAG);
        }
    }

    /// 是否使用平台原生恢复提示（应用内对话框不可用的场景）。
    fn use_native_recovery_prompt(&self) -> bool {
        cfg!(target_os = "android") || self.test_state.test_mode_state.is_some()
    }

    /// 自动保存恢复加载成功后调用（在标准 `MidiParsed` 处理末尾）。
    ///
    /// 把恢复内容明确降级为“未保存工程”，并建立 backup 快照。
    pub(crate) fn finish_autosave_recovery(&mut self) {
        let Some(recovery) = self.autosave_recovery.take() else {
            return;
        };

        // 恢复内容不是用户主动打开的源文件：清空 source，避免 Ctrl+S
        // 把 autosave 当原文件覆盖；同时标记 modified，正常退出时走保存确认。
        downgrade_to_unsaved(
            &mut self.midi_state.current_midi_source,
            &mut self.midi_state.cloud_source,
        );
        {
            let ui = self.window_state.window.ui_mut();
            ui.root_mut().editor.editor_state.data.modified = true;
            ui.set_status_message(Some(tr!("已恢复自动保存，请另存为").to_string()));
        }

        // DEBT-12：backup 只从**已成功载入**的文件复制——损坏的 main 已在
        // `handle_autosave_load_failed` 改名，不会走到这里。
        let (from, to) = plan::backup_sync_direction(recovery.using_backup);
        sync_backup_snapshot(
            from.path(&recovery.main, &recovery.backup),
            &to.path(&recovery.main, &recovery.backup),
        );
        // 本次写入的 main 已确认可信，允许后续轮换覆盖 backup。
        self.autosave_main_trusted
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// 自动保存恢复加载失败时调用（在 `MidiParseError` 处理中）。
    ///
    /// 主文件失败 → 改名 `*-lost-*` 并尝试 backup；备份也失败 → 保留两份
    /// `lost` 文件，提示一次后结束。绝不覆盖损坏文件。
    pub(crate) fn handle_autosave_load_failed(&mut self) {
        let Some(recovery) = self.autosave_recovery.take() else {
            return;
        };

        match plan::after_load_failed(recovery.using_backup, recovery.backup.exists()) {
            LoadFailureNext::FallbackToBackup => {
                let lost = rename_to_unique(&recovery.main, plan::LOST_TAG);
                // 不在这里先弹一次「主文件损坏」：等备份加载结果出来只提示一次。
                tracing::warn!(
                    "主自动保存损坏，改名为 {:?}，尝试备份 {:?}",
                    lost,
                    recovery.backup
                );
                // 损坏的 main 不可信：禁止它在下一次写入时覆盖有效 backup。
                self.autosave_main_trusted
                    .store(false, std::sync::atomic::Ordering::SeqCst);
                self.autosave_recovery = Some(AutosaveRecovery {
                    main: recovery.main,
                    backup: recovery.backup.clone(),
                    using_backup: true,
                });
                self.load_midi_file(recovery.backup);
            }
            LoadFailureNext::ReportLost => {
                let source = if recovery.using_backup {
                    &recovery.backup
                } else {
                    &recovery.main
                };
                let lost = rename_to_unique(source, plan::LOST_TAG);
                self.autosave_main_trusted
                    .store(false, std::sync::atomic::Ordering::SeqCst);
                self.notify_autosave_lost(lost.as_deref().unwrap_or(source));
                tracing::error!("自动保存不可恢复（主/备份均已失败）: {:?}", source);
            }
        }
    }

    /// 最后一道提示：两份文件都不可用。
    ///
    /// 这里不用原生模态框：它发生在 `MidiParseError` 事件处理/后台加载回调链上，
    /// 紧接启动恢复模态框再弹一个原生框会导致 Windows 模态循环卡死（表现为
    /// 提示音后无响应）。改为底边栏状态 + 日志，应用保持可交互。
    fn notify_autosave_lost(&mut self, path: &Path) {
        let message = trf!(
            "自动保存无法恢复，文件已保留：{path}。请从原工程或备份手动恢复。",
            path = path.display()
        );
        tracing::error!("{}", message);
        self.window_state
            .window
            .ui_mut()
            .set_status_message(Some(message));
    }
}

/// 平台原生恢复提示（仅回退路径使用）。
fn ask_recovery_native(candidate: &Path, modified_at: &str) -> bool {
    let restore_label = tr!("恢复自动保存").to_string();
    let discard_label = tr!("不恢复").to_string();
    let result = MessageDialog::new()
        .set_level(MessageLevel::Info)
        .set_title(tr!("检测到未保存的自动保存"))
        .set_description(format!(
            "{}\n\n{}",
            trf!(
                "发现上次会话的自动保存（修改于 {time}）。\n\n恢复不会改动任何已有文件。",
                time = modified_at
            ),
            tr!("按「确定」恢复，按「取消」不恢复。")
        ))
        .set_buttons(MessageButtons::OkCancelCustom(
            restore_label.clone(),
            discard_label.clone(),
        ))
        .show();

    // Windows 未启用 common-controls-v6 时 rfd 会退回标准 OK/Cancel
    // MessageBox，此时返回 `Ok`。恢复是提示框的主操作，`Ok` 也按恢复处理；
    // `Cancel`/自定义“不恢复”才放弃，避免用户点“确定”却被改名。
    let restore = match result {
        MessageDialogResult::Ok => true,
        MessageDialogResult::Custom(label) => label == restore_label,
        MessageDialogResult::Cancel => false,
    };
    tracing::info!(
        "原生恢复提示：{} {:?}",
        if restore { "恢复" } else { "放弃" },
        candidate
    );
    restore
}

/// 把恢复内容降级为「未保存工程」：清空磁盘来源与云来源。
///
/// 返回后调用方必须把 `modified` 置 true——恢复内容不是用户主动保存的工程，
/// Ctrl+S 必须走另存为，正常退出必须走保存确认。
pub(crate) fn downgrade_to_unsaved(
    current_source: &mut Option<PathBuf>,
    cloud_source: &mut Option<crate::runner::inner::CloudSource>,
) {
    *current_source = None;
    *cloud_source = None;
}

/// 复制 backup 快照；失败只记日志（不阻断启动）。
fn sync_backup_snapshot(from: PathBuf, to: &Path) {
    if let Err(e) = std::fs::copy(&from, to) {
        tracing::warn!(
            "自动保存恢复后建立备份失败 {} -> {}: {e}",
            from.display(),
            to.display()
        );
    } else {
        tracing::info!(
            "自动保存恢复完成，已建立备份 {} -> {}",
            from.display(),
            to.display()
        );
    }
}

/// 把文件改名为 `<stem>-<tag>-<unix秒>[-n].lmpj`，永不覆盖已有文件。
fn rename_to_unique(path: &Path, tag: &str) -> Option<PathBuf> {
    rename_to_unique_at(path, tag, unix_now())
}

/// 当前 Unix 秒（取不到时为 0，仅用于命名）。
fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// [`rename_to_unique`] 的可注入时间版本（测试命名冲突用）。
fn rename_to_unique_at(path: &Path, tag: &str, stamp: u64) -> Option<PathBuf> {
    if !path.exists() {
        return None;
    }
    let parent = path.parent()?;
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "autosave".to_string());
    let extension = path
        .extension()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "lmpj".to_string());
    let base = format!("{stem}-{tag}-{stamp}");

    for index in 0..100 {
        let name = if index == 0 {
            format!("{base}.{extension}")
        } else {
            format!("{base}-{index}.{extension}")
        };
        let candidate = parent.join(name);
        if candidate.exists() {
            continue;
        }
        match std::fs::rename(path, &candidate) {
            Ok(()) => {
                tracing::warn!("自动保存文件已保留为 {:?}", candidate);
                return Some(candidate);
            }
            Err(e) => {
                tracing::warn!("自动保存文件改名失败 {:?} -> {:?}: {e}", path, candidate);
                return None;
            }
        }
    }
    None
}

/// 文件修改时刻的本地化展示（取不到时返回占位文案）。
fn modified_at_label(path: &Path) -> String {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map(chrono::DateTime::<chrono::Local>::from)
        .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|_| tr!("未知时间").to_string())
}
