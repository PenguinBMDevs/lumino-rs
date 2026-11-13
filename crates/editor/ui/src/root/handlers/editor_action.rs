//! 编辑器动作与钢琴卷帘上下文菜单处理器
//!
//! 处理 `Message::EditorAction` 与 `Message::PianoRollContextMenu`，
//! 将编辑操作委托给 Editor，并负责播放引擎同步。

use crate::message::EditorAction;
use crate::root::Root;
use lumino_core::tr;
use lumino_message::{ContextMenuTarget, PianoRollContextMenuAction, PianoRollContextMenuItem};

impl Root {
    /// 处理编辑器动作
    ///
    /// 返回 `true` 表示音符数据确实发生了变化。
    pub(crate) fn handle_editor_action(&mut self, action: EditorAction) -> bool {
        puffin::profile_function!();
        // 演奏指示线移动与滚动不修改音符数据，直接返回 false，
        // 避免被误判为脏音轨而触发昂贵的后台重生成。
        let is_playhead_or_scroll = matches!(
            action,
            EditorAction::Scrubbed { .. }
                | EditorAction::IndicatorDragStart { .. }
                | EditorAction::IndicatorDragMove { .. }
                | EditorAction::Scrolled { .. }
        );

        // 编辑拦截：Undo/Redo 在编辑状态下被 Editor::undo/redo 拦截，
        // 这里检测拦截并按 UiConfig 设置显示 Toast 提示用户。
        if matches!(action, EditorAction::Undo | EditorAction::Redo) && self.editor.is_editing() {
            if self.intercept_notification_enabled() {
                self.toast.push(
                    crate::toast::ToastLevel::Warning,
                    tr!("请先完成当前编辑（拖动 / 绘制 / 调整大小）后再执行撤销/重做"),
                );
            }
            tracing::info!(
                "Editor: 拦截 {:?}（toast_enabled={}, edit_state={:?}）",
                action,
                self.intercept_notification_enabled(),
                self.editor.editor_state.interaction.edit_state
            );
            return false;
        }

        let old_tick = self.editor.playback_position;
        {
            puffin::profile_scope!("editor_handle_action");
            self.editor.handle_action(action);
        }
        // 画布 Ctrl+单击（填充桶）→ 取走弹窗请求，打开「分音符填充」覆盖层。
        // Editor 不持有主窗口对话框状态，只能置一次性标志，由此处转成 UI 状态。
        if self.editor.take_fill_division_dialog_request() {
            self.open_fill_division_dialog();
        }
        let new_tick = self.editor.playback_position;

        // 检查播放位置是否变化
        if (old_tick - new_tick).abs() > f32::EPSILON
            && let Some(manager) = &mut self.playback.manager
        {
            manager.seek(new_tick);
        }

        if is_playhead_or_scroll {
            return false;
        }

        // 检查音符数据是否变化
        let notes_changed = self.editor.notes_changed();
        if notes_changed {
            puffin::profile_scope!("update_playback_notes_on_release");
            self.update_playback_notes();
            self.editor.clear_notes_changed();
        }
        notes_changed
    }

    /// 打开「分音符填充」面板（输入框预填当前档位）
    ///
    /// 触发路径：画布 Ctrl+单击（`Editor` 置请求位 → 此处取走）；
    /// 以及音符画悬浮工具条「颜料桶」条目**启用后再次点击**
    /// （`ToolPanelItemSettingsRequested`）。
    ///
    /// 该面板已从全屏居中弹窗重构为贴图标上方的**工具栏小面板**（由
    /// `root/draw_toolbar.rs` 渲染），故此处需确保音符画工具箱处于展开态——
    /// 否则面板无处锚定；同时与其他工具设置下拉（画刷 / 形状）互斥。
    pub(crate) fn open_fill_division_dialog(&mut self) {
        let current = self.editor.fill_division();
        self.state.fill_division_dialog.is_open = true;
        self.state.fill_division_dialog.value = current.map(|n| n.to_string()).unwrap_or_default();
        self.toolbar.tool_panel_open = true;
        self.toolbar.brush_dropdown_open = false;
        self.toolbar.shape_dropdown_open = false;
    }

    /// 处理钢琴卷帘右键上下文菜单动作
    pub(crate) fn handle_piano_roll_context_menu(&mut self, action: PianoRollContextMenuAction) {
        match action {
            PianoRollContextMenuAction::Open { position, target } => {
                let pos = lumino_ui_core::geom::Point::new(position.x, position.y);
                match target {
                    // 图形目标：右键命中哪个图形就作用于它。若它**已在选中集内**则保持
                    // 整个多选不变（菜单「删除」作用于既有批量选区），与音符目标的语义一致。
                    ContextMenuTarget::DrawnShape | ContextMenuTarget::PendingDrawnShape => {
                        if let Some(id) = self.editor.drawn_shape_at_screen(pos) {
                            self.editor.select_drawn_shape(id);
                        }
                    }
                    // 音符目标：右键点击音符且该音符不在选中集合时，先将其设为唯一选中。
                    // 使菜单的 删除/剪切/复制 作用于"右键目标"，与 Delete 键
                    // "有选中集合即删除选中集合"的语义一致——否则右键一个未选中的
                    // 音符，菜单"删除"会删掉旧的批量选区。
                    ContextMenuTarget::Notes => {
                        if let Some((index, _)) = self.editor.hit_test_note(pos)
                            && !self.editor.is_note_selected(index)
                        {
                            self.editor.selection_clear();
                            self.editor.selection_insert(index);
                        }
                    }
                }
                self.editor.context_menu.open(pos, target);
            }
            PianoRollContextMenuAction::Close => {
                self.editor.context_menu.close();
            }
            PianoRollContextMenuAction::ItemClicked(item) => {
                // 面板锚点要先取：`close()` 会清空 position
                let menu_pos = self.editor.context_menu.position;
                self.editor.context_menu.close();
                match item {
                    PianoRollContextMenuItem::BatchEdit => {
                        crate::event::emit(crate::event::Event::Window(
                            crate::event::window::Event::open_batch_edit_dialog(),
                        ));
                    }
                    PianoRollContextMenuItem::Cut => {
                        let _ = self.handle_editor_action(EditorAction::Cut);
                    }
                    PianoRollContextMenuItem::Copy => {
                        let _ = self.handle_editor_action(EditorAction::Copy);
                    }
                    PianoRollContextMenuItem::Paste => {
                        let _ = self.handle_editor_action(EditorAction::Paste);
                    }
                    PianoRollContextMenuItem::Delete => {
                        let _ = self.handle_editor_action(EditorAction::DeletePressed);
                    }
                    PianoRollContextMenuItem::DeleteDrawnShape => {
                        // 与 Delete 键同一条路径：鼠标工具下会优先删除选中的绘制图形
                        let _ = self.handle_editor_action(EditorAction::DeletePressed);
                    }
                    PianoRollContextMenuItem::ReturnToDrawer => {
                        // DEBT-14：只对「从抽屉 Insert 进来的 pending」有效；
                        // 退回成功才移动数据，失败保留 pending（见 return_pending_to_drawer）。
                        let index = self
                            .editor
                            .editor_state
                            .shape_select
                            .selected_shapes()
                            .filter_map(|shape| shape.pending)
                            .find_map(|reference| match reference {
                                lumino_editor_state::PendingShapeRef::Composed(index) => {
                                    Some(index)
                                }
                                _ => None,
                            });
                        match index.and_then(|index| self.editor.return_pending_to_drawer(index)) {
                            Some(drawer) => {
                                self.state.shape_drawer_return = Some(drawer);
                            }
                            None => {
                                self.toast.push(
                                    crate::toast::ToastLevel::Warning,
                                    tr!("退回失败：请选中一个从抽屉插入的待确认形状"),
                                );
                            }
                        }
                    }
                    PianoRollContextMenuItem::LiveShape => {
                        if !self.editor.live_selected_shapes() {
                            self.toast.push(
                                crate::toast::ToastLevel::Warning,
                                tr!("转为活形状失败：请选中两个及以上待确认图形（已 Bake 只读）"),
                            );
                        }
                    }
                    PianoRollContextMenuItem::SplitLiveShape
                    | PianoRollContextMenuItem::SplitJoinedShape => {
                        let _ = self.editor.split_composed_shape();
                    }
                    PianoRollContextMenuItem::JoinShapes => {
                        if !self.editor.join_selected_shapes() {
                            self.toast.push(
                                crate::toast::ToastLevel::Warning,
                                tr!("连接图形失败：请选中至少两个待确认图形（已 Bake 只读）"),
                            );
                        }
                    }
                    PianoRollContextMenuItem::MergeShapes => {
                        if !self.editor.merge_selected_shapes() {
                            self.toast.push(
                                crate::toast::ToastLevel::Warning,
                                tr!("门控合并失败：请恰好选中两个待确认图形（已 Bake 只读）"),
                            );
                        }
                    }
                    PianoRollContextMenuItem::EditShape => {
                        if self.editor.enable_shape_edit() {
                            let message = if self.editor.is_shape_edit_armed() {
                                tr!("已进入编辑图形：角=缩放，边=拉伸，角外=旋转，中心=移动")
                            } else {
                                tr!("已退出编辑图形")
                            };
                            self.toast.push(crate::toast::ToastLevel::Info, message);
                        } else {
                            self.toast.push(
                                crate::toast::ToastLevel::Warning,
                                tr!("编辑图形失败：请选中一个单支待确认组合（已 Bake 只读）"),
                            );
                        }
                    }
                    PianoRollContextMenuItem::PaintAreas => {
                        if self.editor.begin_area_paint() {
                            self.toast.push(
                                crate::toast::ToastLevel::Info,
                                tr!("区域着色已开启：点击形状内部上色，切工具退出"),
                            );
                        } else {
                            self.toast.push(
                                crate::toast::ToastLevel::Warning,
                                tr!("区域着色失败：请选中一个待确认闭合形状"),
                            );
                        }
                    }
                    PianoRollContextMenuItem::MaskToggle => {
                        if self.editor.toggle_mask_selected() {
                            self.toast.push(
                                crate::toast::ToastLevel::Info,
                                tr!("已切换遮罩（仅预览；✓ 后固定）"),
                            );
                        } else {
                            self.toast.push(
                                crate::toast::ToastLevel::Warning,
                                tr!("遮罩失败：请选中一个待确认形状"),
                            );
                        }
                    }
                    PianoRollContextMenuItem::MaskSettings => {
                        // 菜单坐标是 canvas 局部，覆盖层面板用窗口坐标
                        let anchor = menu_pos
                            .map(|pos| {
                                let canvas = &self.editor.editor_state.canvas;
                                lumino_ui_core::geom::Point::new(
                                    pos.x + canvas.offset_x,
                                    pos.y + canvas.offset_y,
                                )
                            })
                            .unwrap_or(lumino_ui_core::geom::Point::new(320.0, 200.0));
                        self.open_mask_panel(anchor);
                    }
                    PianoRollContextMenuItem::MaskApply => {
                        let model_id = self
                            .editor
                            .editor_state
                            .shape_select
                            .selected_shapes()
                            .find_map(|shape| shape.model_id);
                        let applied = model_id.and_then(|id| self.editor.apply_mask(id));
                        if applied.is_some() {
                            self.toast.push(
                                crate::toast::ToastLevel::Success,
                                tr!("已实体化遮罩：可见片段写成新形状"),
                            );
                        } else {
                            self.toast.push(
                                crate::toast::ToastLevel::Warning,
                                tr!("实体化失败：请选中一个带遮罩的已 Bake 形状"),
                            );
                        }
                    }
                    PianoRollContextMenuItem::ShapeList => {
                        let _ = self.editor.open_shape_list();
                    }
                    PianoRollContextMenuItem::SymmetryVertical => {
                        if let Err(reason) = self.editor.toggle_symmetry(true) {
                            self.toast.push(
                                crate::toast::ToastLevel::Warning,
                                lumino_ui_editor::sym_reject_message(reason),
                            );
                        }
                    }
                    PianoRollContextMenuItem::SymmetryHorizontal => {
                        if let Err(reason) = self.editor.toggle_symmetry(false) {
                            self.toast.push(
                                crate::toast::ToastLevel::Warning,
                                lumino_ui_editor::sym_reject_message(reason),
                            );
                        }
                    }
                    PianoRollContextMenuItem::Straighten => {
                        let _ = self.editor.set_straighten(true);
                    }
                    PianoRollContextMenuItem::StraightenOff => {
                        let _ = self.editor.set_straighten(false);
                    }
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
                    PianoRollContextMenuItem::MouldShapes => match self.editor.mould_selected() {
                        Ok(()) => {
                            self.toast.push(
                                crate::toast::ToastLevel::Info,
                                tr!("已切出内/外片；✓ 后才写入音符"),
                            );
                        }
                        Err(lumino_ui_editor::MouldReject::Selection) => {
                            self.toast.push(
                                crate::toast::ToastLevel::Warning,
                                tr!("饼干模失败：请选中 1 个待确认形状（或 2 个：刀 + 面团）"),
                            );
                        }
                        Err(lumino_ui_editor::MouldReject::NoWalls) => {
                            self.toast.push(
                                crate::toast::ToastLevel::Warning,
                                tr!("饼干模失败：先用鼠标工具选中要当刀的音符，再切到鼠标工具"),
                            );
                        }
                        Err(lumino_ui_editor::MouldReject::NoGap) => {
                            self.toast.push(
                                crate::toast::ToastLevel::Warning,
                                tr!("饼干模失败：刀的同一 key 行需要至少两条音符（中间才是空隙）"),
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
                    PianoRollContextMenuItem::BetweenCreate => {
                        if self.editor.create_between_selected() {
                            self.toast.push(
                                crate::toast::ToastLevel::Info,
                                tr!("已生成补间（首 + 步 + 末）；✓ 后才写入音符"),
                            );
                        } else {
                            self.toast.push(
                                crate::toast::ToastLevel::Warning,
                                tr!("补间失败：请恰好选中两个待确认形状/曲线"),
                            );
                        }
                    }
                    PianoRollContextMenuItem::CopyAlong => {
                        if self.editor.copy_along_selected() {
                            if let Some(summary) = self.editor.copy_along_summary() {
                                self.toast.push(
                                    crate::toast::ToastLevel::Info,
                                    format!("沿路径复制：{summary}"),
                                );
                            }
                        } else {
                            self.toast.push(
                                crate::toast::ToastLevel::Warning,
                                tr!("沿路径复制失败：请选中一个待确认形状/曲线；组合结果需先 ✓"),
                            );
                        }
                    }
                    PianoRollContextMenuItem::VelocityUp => {
                        if self.editor.velocity_up_selected() {
                            self.toast.push(
                                crate::toast::ToastLevel::Info,
                                tr!("已应用力度渐强；✓ 后才写入音符"),
                            );
                        } else {
                            self.toast.push(
                                crate::toast::ToastLevel::Warning,
                                tr!("力度渐强失败：请选中一个待确认形状/曲线；组合结果需先 ✓"),
                            );
                        }
                    }
                    PianoRollContextMenuItem::CopyShapeText => {
                        let mut models = self.editor.selected_shape_models();
                        // 待确认图形没有 ShapeStore 模型，但有几何：按当前几何临时构造，
                        // 让用户可以在 √ / Bake 之前先复制分享文本。
                        if models.is_empty()
                            && let Some(pending) = self.editor.selected_pending_shape_model()
                        {
                            models.push(pending);
                        }
                        if models.is_empty() {
                            self.toast.push(
                                crate::toast::ToastLevel::Warning,
                                tr!("没有可分享的形状，请先用鼠标工具选中已确认或待确认图形"),
                            );
                        } else {
                            let ppq = self.editor.editor_state.view.ppq;
                            match lumino_core::shape::share::encode_shapes(&models, ppq) {
                                Ok(text) => match lumino_platform::clipboard::set_text(text) {
                                    Ok(()) => self.toast.push(
                                        crate::toast::ToastLevel::Success,
                                        tr!("已复制形状分享文本"),
                                    ),
                                    Err(error) => self.toast.push(
                                        crate::toast::ToastLevel::Error,
                                        format!("{}: {error}", tr!("复制到剪贴板失败")),
                                    ),
                                },
                                Err(error) => self.toast.push(
                                    crate::toast::ToastLevel::Error,
                                    format!("{}: {error}", tr!("形状分享编码失败")),
                                ),
                            };
                        }
                    }
                    PianoRollContextMenuItem::PasteShapeText => {
                        match lumino_platform::clipboard::get_text() {
                            Ok(text) => match self.editor.paste_shapes_from_text(&text) {
                                Ok(count) => self.toast.push(
                                    crate::toast::ToastLevel::Success,
                                    format!("{}: {count}", tr!("已粘贴形状")),
                                ),
                                Err(error) => self.toast.push(
                                    crate::toast::ToastLevel::Error,
                                    format!("{}: {error}", tr!("形状分享粘贴失败")),
                                ),
                            },
                            Err(error) => self.toast.push(
                                crate::toast::ToastLevel::Error,
                                format!("{}: {error}", tr!("读取剪贴板失败")),
                            ),
                        };
                    }
                    PianoRollContextMenuItem::SelectAll => {
                        let _ = self.handle_editor_action(EditorAction::SelectAll);
                    }
                }
            }
        }
    }
}
