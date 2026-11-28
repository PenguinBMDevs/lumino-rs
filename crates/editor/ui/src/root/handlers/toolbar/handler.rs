//! ToolbarHandler 结构体定义与主入口

use crate::message::Message;
use crate::root::Root;
use crate::root::handlers::MessageHandler;

/// 工具栏消息处理器
///
/// 注意：此处理器处理工具栏事件，但对于播放控制，
/// 它直接将消息转发给专门的处理器，而不是递归调用 update。
pub struct ToolbarHandler;

impl ToolbarHandler {
    /// 创建一个工具栏消息处理器
    pub fn new() -> Self {
        Self
    }

    fn handle_toolbar_event(&self, root: &mut Root, event: crate::toolbar::Event) {
        // PPQ 编辑确认：成功才收起编辑态；失败保留输入与错误，供用户继续改。
        if matches!(event, crate::toolbar::Event::PpqEditConfirmed) {
            // 已不在编辑态：重复的确认信号必须丢弃。失焦信号可能与其它消息
            // 同帧到达，二次提交会把清理后的空缓冲当成非法输入留下错误。
            if !root.toolbar.ppq_editing {
                return;
            }
            if Self::confirm_ppq_edit(root) {
                root.toolbar.update(event);
            }
            return;
        }

        // BPM 编辑确认：与 PPQ 同一契约（失败保留输入与错误，供用户继续改）。
        if matches!(event, crate::toolbar::Event::BpmEditConfirmed) {
            if !root.toolbar.bpm_editing {
                return;
            }
            if Self::confirm_bpm_edit(root) {
                root.toolbar.update(event);
            }
            return;
        }

        // 编辑锚点与临时浮层：必须早于 `toolbar.update` 生效。
        Self::prepare_event_state(root, &event);

        // 更新工具栏自身状态
        root.toolbar.update(event.clone());

        // 同步画刷配置到编辑器（绘制时按当前全局粗细/音轨分配落笔）
        root.editor.brush = root.toolbar.brush.clone();

        // 处理播放控制 - 直接执行，不通过消息循环
        self.handle_toolbar_playback(root, &event);

        // 工具状态与首次选中帮助 tip
        self.sync_tool_state_and_help(root, &event);

        // 其余工具栏动作：精度 / 滚动 / 撤销 / 量化 / 悬停
        self.dispatch_toolbar_actions(root, &event);
    }

    /// 分派前的状态准备：锁定 BPM 编辑锚点、收起「分音符填充」临时浮层。
    ///
    /// 两者都必须早于 `toolbar.update`：锚点记录的是「用户开始编辑那一刻」的播放
    /// 位置，浮层收起则不应看见本次事件写入后的状态。
    fn prepare_event_state(root: &mut Root, event: &crate::toolbar::Event) {
        // 进入 BPM 编辑时锁定播放位置：播放中编辑时播放头继续前进，提交必须
        // 修改用户一开始看到的那条 tempo 点，而不是「提交时刻」生效的另一条。
        if matches!(event, crate::toolbar::Event::BpmEditToggled(_)) {
            root.toolbar.bpm_edit_anchor_tick = root.editor.playback_position;
        }

        // 颜料桶「分音符填充」面板打开时，除再次点击颜料桶与悬停外，其余工具栏操作
        // 先关闭它——与画刷 / 形状下拉同一「外部关闭」语义（面板内值变更 / 确认 / 取消
        // 走 `FillDivision` 消息，不经此处，故面板内交互不受影响）。
        //
        // 齿轮（`OpenDrawSettingsDialog`）**不例外**：它是独立 OS 窗口，与窗口内浮层
        // 谈不上"同屏打架"，但三块窗口内面板本就是"外部一动作即收起"的临时浮层
        // （输入值存于 `state`，收起不丢数据）。给齿轮单开例外只会让"同一动作下三块
        // 面板行为不一致"，得不偿失——统一比省一次点击重要。
        if root.state.fill_division_dialog.is_open
            && !matches!(
                event,
                crate::toolbar::Event::ToolPanelItemSettingsRequested(
                    crate::toolbar::ToolPanelItem::FillBucket
                ) | crate::toolbar::Event::ButtonHovered(_)
            )
        {
            root.state.fill_division_dialog.is_open = false;
        }
    }

    /// 同步工具状态，并在工具首次选中时弹一次帮助 tip。
    fn sync_tool_state_and_help(&self, root: &mut Root, event: &crate::toolbar::Event) {
        // 同步工具状态
        self.sync_toolbar_tool_state(root, event);
        // 工具首次选中 → 弹一次帮助 tip（DRAW-038）
        match event {
            crate::toolbar::Event::ToolSelected(tool) => root.maybe_show_help_tip(*tool),
            crate::toolbar::Event::ToolPanelItemSelected(_)
            | crate::toolbar::Event::ToolPanelItemSettingsRequested(_) => {
                let tool = root.toolbar.current_tool;
                root.maybe_show_help_tip(tool);
            }
            _ => {}
        }
    }

    /// 逐项分派与工具栏动作对应的处理器（精度、滚动、撤销、量化、翻转、悬停等）。
    fn dispatch_toolbar_actions(&self, root: &mut Root, event: &crate::toolbar::Event) {
        // 同步精度设置
        self.sync_toolbar_precision(root, event);

        // 同步自动滚动模式（在 toolbar 更新之后）
        self.sync_auto_scroll_mode(root, event);

        // 同步洋葱皮显示范围（未接入的「指定音轨」在此提示）
        self.sync_onion_skin_mode(root, event);

        // 处理撤销/重做
        self.handle_toolbar_undo_redo(root, event);

        // 处理量化
        self.handle_toolbar_quantize(root, event);

        // 处理音符变速
        self.handle_toolbar_speed_change(root, event);

        // 处理协作对话框
        self.handle_toolbar_collaboration(root, event);

        // 处理内存监控对话框
        self.handle_toolbar_memory_monitor(root, event);

        // 处理「音符画设置」对话框（悬浮条右端齿轮）
        self.handle_toolbar_draw_settings(root, event);
        // 处理「形状抽屉」独立窗口（DRAW-033B0；正式发布构建不编译）
        #[cfg(lumino_shape_drawer_entry)]
        self.handle_toolbar_shape_drawer(root, event);
        // 处理「撤销历史」独立窗口（DRAW-035）
        self.handle_toolbar_history(root, event);

        // 处理录制
        self.handle_toolbar_recording(root, event);

        // 处理垂直翻转
        self.handle_toolbar_flip_vertical(root, event);

        // 处理水平翻转
        self.handle_toolbar_flip_horizontal(root, event);

        // 处理移调
        self.handle_toolbar_transpose(root, event);

        // 处理连奏
        self.handle_toolbar_tie(root, event);

        // 处理分割/合并
        self.handle_toolbar_split_glue(root, event);

        // 工具栏按钮悬停：驱动底部状态栏左侧描述区
        if let crate::toolbar::Event::ButtonHovered(label) = event {
            root.statusbar.set_hover_label(*label);
        }
    }

    /// 确认 PPQ 编辑：整数表达式求值 → 写入编辑器 → 清除编辑状态
    ///
    /// 返回 `false` 表示提交失败：调用方不得收起编辑态，输入框内容与错误提示保留。
    fn confirm_ppq_edit(root: &mut Root) -> bool {
        // MIDI 规范：PPQ 范围 24-32767
        match lumino_ui_core::widget::eval_int_field(&root.toolbar.ppq_edit_buffer, 24, 32767) {
            Ok(ppq) => {
                root.toolbar.ppq_edit_error = None;
                root.toolbar.ppq_editing = false;
                root.toolbar.ppq_edit_buffer.clear();
                root.set_ppq(ppq as u16);
                tracing::info!("PPQ 已更新为 {}", ppq);
                true
            }
            Err(error) => {
                tracing::warn!(
                    "PPQ 输入非法 {:?}（{error}），保留编辑态",
                    root.toolbar.ppq_edit_buffer
                );
                root.toolbar.ppq_edit_error = Some(error);
                false
            }
        }
    }

    /// 确认 BPM 编辑：十进制数校验 → 写入进入编辑时锚定的 tempo 点 → 清除编辑状态
    ///
    /// 返回 `false` 表示提交失败：调用方不得收起编辑态，输入框内容与错误提示保留。
    fn confirm_bpm_edit(root: &mut Root) -> bool {
        let max_bpm = root.editor.velocity_panel.tempo_max_bpm;
        match Self::parse_bpm(&root.toolbar.bpm_edit_buffer, max_bpm) {
            Ok(bpm) => {
                root.toolbar.bpm_edit_error = None;
                root.toolbar.bpm_editing = false;
                root.toolbar.bpm_edit_buffer.clear();
                Self::apply_bpm(root, bpm);
                tracing::info!("BPM 已更新为 {}", bpm);
                true
            }
            Err(error) => {
                tracing::warn!(
                    "BPM 输入非法 {:?}（{error}），保留编辑态",
                    root.toolbar.bpm_edit_buffer
                );
                root.toolbar.bpm_edit_error = Some(error);
                false
            }
        }
    }

    /// 解析并校验 BPM 输入（十进制数，范围与 Tempo 面板一致）。
    fn parse_bpm(src: &str, max_bpm: f64) -> Result<f64, String> {
        use lumino_core::shape::expr::fmt_number;

        let bpm: f64 = src
            .trim()
            .parse()
            .map_err(|_| "请输入数字（如 120 或 120.5）".to_string())?;
        if !bpm.is_finite() {
            return Err("BPM 必须是有限数字".to_string());
        }
        if !(crate::editor::velocity::layout::TEMPO_BPM_MIN..=max_bpm).contains(&bpm) {
            return Err(format!(
                "数值 {} 超出范围 {}..={}",
                fmt_number(bpm),
                fmt_number(crate::editor::velocity::layout::TEMPO_BPM_MIN),
                fmt_number(max_bpm),
            ));
        }
        Ok(bpm)
    }

    /// 把 BPM 写入仪表盘当前显示的那个 tempo 点。
    ///
    /// - 显示值 = `bpm_edit_anchor_tick`（进入编辑时的播放位置）之前最近的速度点；
    ///   没有点 → 新建 tick 0 的显式速度点（空速度表时仪表盘显示默认 120，
    ///   编辑它就是把默认速度落到工程里）；
    /// - 值未变化不产生历史记录，避免「点一下又点回来」污染撤销栈。
    fn apply_bpm(root: &mut Root, bpm: f64) {
        let tick = root.toolbar.bpm_edit_anchor_tick;
        let active = {
            let points = &root.editor.editor_state.data.tempo_points;
            crate::toolbar::tempo_index_at_tick(points, tick)
                .map(|index| (index, points[index].bpm))
        };
        match active {
            None => {
                root.editor.push_history();
                root.editor.editor_state.data.add_tempo_point(0.0, bpm);
                root.update_playback_bpm();
            }
            Some((index, old_bpm)) if (old_bpm - bpm).abs() > 1e-9 => {
                root.editor.push_history();
                root.editor.editor_state.data.set_tempo_point(index, bpm);
                root.update_playback_bpm();
            }
            Some(_) => {}
        }
    }
}

impl Default for ToolbarHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageHandler for ToolbarHandler {
    fn handle(&mut self, root: &mut Root, msg: Message) -> Option<Message> {
        match msg {
            Message::Toolbar(event) => {
                self.handle_toolbar_event(root, event);
                None
            }
            other => Some(other),
        }
    }
}
