//! 自动保存恢复状态机：启动询问、损坏保命、备份回退。
//!
//! 恢复必须作为“未保存工程”载入：
//! - 清空 `current_midi_source` / 云端来源；
//! - 标记 `modified = true`；
//! - 不覆盖 autosave，也不把 autosave 当作可 Ctrl+S 覆盖的源文件。

use std::path::{Path, PathBuf};

use lumino_core::{tr, trf};
use lumino_platform::dialog::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};

use crate::runner::RunnerInner;

use super::{autosave_backup_path, autosave_path};

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
    pub(crate) fn prompt_autosave_recovery(&mut self) {
        let main = autosave_path();
        let backup = autosave_backup_path();
        let (candidate, using_backup) = if main.exists() {
            (main.clone(), false)
        } else if backup.exists() {
            (backup.clone(), true)
        } else {
            return;
        };

        let modified_at = modified_at_label(&candidate);
        let restore_label = tr!("恢复自动保存").to_string();
        let discard_label = tr!("不恢复").to_string();
        let result = MessageDialog::new()
            .set_level(MessageLevel::Info)
            .set_title(tr!("检测到未保存的自动保存"))
            .set_description(trf!(
                "发现上次会话的自动保存（修改于 {time}）。\n\n恢复不会改动任何已有文件。",
                time = modified_at
            ))
            .set_buttons(MessageButtons::OkCancelCustom(
                restore_label.clone(),
                discard_label.clone(),
            ))
            .show();

        let restore = matches!(
            result,
            MessageDialogResult::Custom(label) if label == restore_label
        );
        if restore {
            tracing::info!("用户选择恢复自动保存: {:?}", candidate);
            self.autosave_recovery = Some(AutosaveRecovery {
                main,
                backup,
                using_backup,
            });
            self.load_midi_file(candidate);
        } else {
            tracing::info!("用户放弃恢复自动保存，改名保留: {:?}", candidate);
            rename_to_unique(&main, "declined");
            rename_to_unique(&backup, "declined");
        }
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
        self.midi_state.current_midi_source = None;
        self.midi_state.cloud_source = None;
        {
            let ui = self.window_state.window.ui_mut();
            ui.root_mut().editor.editor_state.data.modified = true;
            ui.set_status_message(Some(tr!("已恢复自动保存，请另存为").to_string()));
        }

        let (from, to) = if recovery.using_backup {
            // 主文件损坏/缺失时已改名为 lost；用备份重新建立主槽位。
            (recovery.backup.clone(), recovery.main.clone())
        } else {
            // 正常恢复：当前主文件就是上次启动快照，复制为 backup。
            (recovery.main.clone(), recovery.backup.clone())
        };
        if let Err(e) = std::fs::copy(&from, &to) {
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

    /// 自动保存恢复加载失败时调用（在 `MidiParseError` 处理中）。
    ///
    /// 主文件失败 → 改名 `*-lost-*` 并尝试 backup；备份也失败 → 保留两份
    /// `lost` 文件，提示一次后结束。绝不覆盖损坏文件。
    pub(crate) fn handle_autosave_load_failed(&mut self) {
        let Some(recovery) = self.autosave_recovery.take() else {
            return;
        };

        if recovery.using_backup {
            let lost = rename_to_unique(&recovery.backup, "lost");
            notify_autosave_lost(lost.as_deref().unwrap_or(&recovery.backup));
            return;
        }

        let lost = rename_to_unique(&recovery.main, "lost");
        if recovery.backup.exists() {
            // 不在这里先弹一次「主文件损坏」：等备份加载结果出来只提示一次。
            tracing::warn!(
                "主自动保存损坏，改名为 {:?}，尝试备份 {:?}",
                lost,
                recovery.backup
            );
            self.autosave_recovery = Some(AutosaveRecovery {
                main: recovery.main,
                backup: recovery.backup.clone(),
                using_backup: true,
            });
            self.load_midi_file(recovery.backup);
            return;
        }

        notify_autosave_lost(lost.as_deref().unwrap_or(&recovery.main));
        tracing::error!("自动保存主文件损坏且没有可用备份: {:?}", recovery.main);
    }
}

/// 把文件改名为 `<stem>-<tag>-<unix秒>[-n].lmpj`，永不覆盖已有文件。
fn rename_to_unique(path: &Path, tag: &str) -> Option<PathBuf> {
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
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
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

/// 最后一道提示：两份文件都不可用。
fn notify_autosave_lost(path: &Path) {
    let _ = MessageDialog::new()
        .set_level(MessageLevel::Warning)
        .set_title(tr!("自动保存无法恢复"))
        .set_description(trf!(
            "自动保存文件无法打开，已改名保留：\n\n{path}\n\n请从原工程或备份手动恢复。",
            path = path.display()
        ))
        .set_buttons(MessageButtons::Ok)
        .show();
}

/// 文件修改时刻的本地化展示（取不到时返回占位文案）。
fn modified_at_label(path: &Path) -> String {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map(chrono::DateTime::<chrono::Local>::from)
        .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|_| tr!("未知时间").to_string())
}
