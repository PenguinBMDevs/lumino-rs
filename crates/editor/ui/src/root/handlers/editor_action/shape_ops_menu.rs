//! 形状组合类右键命令：切割 / 饼干模 / 角度柔化（DRAW-025、DEBT-17）
//!
//! 从 `editor_action.rs` 拆出（文件长度纪律）；只负责把 Editor 结果翻译成 toast。

use lumino_core::{tr, trf};
use lumino_message::PianoRollContextMenuItem;

use crate::root::Root;

impl Root {
    /// 刀失效等一次性通知 → toast（DEBT-17）
    pub(crate) fn drain_mould_notice(&mut self) {
        let Some(notice) = self.editor.take_mould_notice() else {
            return;
        };
        match notice {
            lumino_ui_editor::MouldNotice::Expired => {
                self.toast.push(
                    crate::toast::ToastLevel::Warning,
                    tr!("刀已失效：音符或形状已改变，请重新设刀"),
                );
            }
        }
    }

    /// 处理切割 / 设刀 / 饼干模 / 角度柔化
    pub(crate) fn handle_shape_ops_menu(&mut self, item: PianoRollContextMenuItem) {
        match item {
            PianoRollContextMenuItem::SliceCut => {
                if self.editor.begin_slice_cut() {
                    self.toast.push(
                        crate::toast::ToastLevel::Info,
                        tr!("拖出一条切割线：松手即切（Shift 自由角度）"),
                    );
                } else {
                    self.toast.push(
                        crate::toast::ToastLevel::Warning,
                        tr!("切割失败：请选中一个待确认形状/曲线；已 Bake 形状不可再切"),
                    );
                }
            }
            PianoRollContextMenuItem::MouldSetKnife => {
                match self.editor.mould_set_knife() {
                    Ok(info) => {
                        let text = match info.notes {
                            Some(notes) => trf!(
                                "已设为刀：{notes} 条音符 · {rows} 行（Esc 取消）",
                                notes = notes,
                                rows = info.rows
                            ),
                            None => trf!(
                                "已设为刀：{rings} 环 · {rows} 行（Esc 取消）",
                                rings = info.rings,
                                rows = info.rows
                            ),
                        };
                        self.toast.push(crate::toast::ToastLevel::Info, text);
                        if !info.can_cut {
                            self.toast.push(
                                crate::toast::ToastLevel::Warning,
                                tr!("刀在每一行都不足两段，切不出内片；请补一条同 key 音符或再加一个环"),
                            );
                        }
                    }
                    Err(_) => {
                        self.toast.push(
                            crate::toast::ToastLevel::Warning,
                            tr!("设刀失败：先选中音符或待确认形状"),
                        );
                    }
                }
            }
            PianoRollContextMenuItem::MouldClearKnife => {
                if self.editor.mould_clear_knife() {
                    self.toast
                        .push(crate::toast::ToastLevel::Info, tr!("已取消刀"));
                } else {
                    self.toast
                        .push(crate::toast::ToastLevel::Info, tr!("当前没有刀"));
                }
            }
            PianoRollContextMenuItem::MouldShapes => match self.editor.mould_selected() {
                Ok(()) => {
                    self.toast.push(
                        crate::toast::ToastLevel::Info,
                        tr!("已切出内/外片；确认后才写入音符"),
                    );
                }
                Err(lumino_ui_editor::MouldReject::Selection) => {
                    self.toast.push(
                        crate::toast::ToastLevel::Warning,
                        tr!("饼干模失败：请选中 1 个待确认形状作为面团"),
                    );
                }
                Err(lumino_ui_editor::MouldReject::NoKnife) => {
                    self.toast.push(
                        crate::toast::ToastLevel::Warning,
                        tr!("饼干模失败：先右键「以选中内容为刀」设置刀"),
                    );
                }
                Err(lumino_ui_editor::MouldReject::NoKnifeSource) => {
                    self.toast.push(
                        crate::toast::ToastLevel::Warning,
                        tr!("设刀失败：先选中音符或待确认形状"),
                    );
                }
                Err(lumino_ui_editor::MouldReject::KnifeStale) => {
                    self.toast.push(
                        crate::toast::ToastLevel::Warning,
                        tr!("饼干模失败：刀已失效（音符或形状已改变），请重新设刀"),
                    );
                }
                Err(lumino_ui_editor::MouldReject::SameShape) => {
                    self.toast.push(
                        crate::toast::ToastLevel::Warning,
                        tr!("饼干模失败：刀与面团不能是同一个形状"),
                    );
                }
                Err(lumino_ui_editor::MouldReject::NoGap) => {
                    self.toast.push(
                        crate::toast::ToastLevel::Warning,
                        tr!("饼干模失败：刀的同一 key 行需要至少两段墙（中间才是空隙）"),
                    );
                }
            },
            PianoRollContextMenuItem::RoundCorners => {
                if self.editor.begin_round_corners() {
                    self.toast.push(
                        crate::toast::ToastLevel::Info,
                        tr!("角度柔化：拖角点调半径（Esc 退出）"),
                    );
                } else {
                    self.toast.push(
                        crate::toast::ToastLevel::Warning,
                        tr!("角度柔化失败：请选中一个待确认折线形状（至少 3 个角点）"),
                    );
                }
            }
            _ => {}
        }
    }
}
