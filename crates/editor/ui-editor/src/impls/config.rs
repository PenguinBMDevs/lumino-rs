//! Editor 配置相关方法 — 简单的 getter/setter/delegate
//!
//! 包含：工具切换、光标/画布状态、总 ticks 和 PPQ、音符变更标志

use crate::Editor;
use lumino_core::BrushConfig;
use lumino_message::Tool;

impl Editor {
    /// 设置当前工具（委托到 editor_state）
    ///
    /// 切到非曲线工具时自动关闭曲线工具颜料桶填充模式；
    /// 切到非形状工具时自动关闭形状工具颜料桶填充模式（均为工具附属开关）。
    ///
    /// ⚠️ 待确认的绘制内容（曲线路径 / 待确认图形 / 画刷笔画 / 文本框）**不因切换工具丢失**
    /// ——见 `EditorState::set_tool` 只收敛交互手势、保留产物。切到「鼠标工具」
    /// （`Tool::ShapeSelect`）时同样**只保留、不固化**：用户没按 √ 就不该生成音符。
    ///
    /// 但「保留」必须同时「可操作」：待确认几何在鼠标工具下照旧渲染，故切换时把它
    /// 镜像进图形选中域（`rebuild_pending_mirrors`）——否则用户看到图案却框不中、
    /// 拖不动（看得见却抓不住）。离开鼠标工具即撤掉镜像（其余工具由 owning 预览渲染）。
    ///
    /// # 编辑态映射（DRAW-048，口径 B）
    ///
    /// | 切到 | 待确认几何 | 鼠标工具的临时会话 |
    /// |---|---|---|
    /// | 拥有者绘制工具（形状 / 曲线 / 画笔 / 文字） | 该工具自己的待确认几何可见且可编辑 | 收口 |
    /// | 鼠标工具（`Tool::ShapeSelect`） | 四类都可见，可选中 / 拖拽 | 可开启 |
    /// | 其他工具 | 隐藏，不注册命中 | 收口 |
    ///
    /// 会话（角度柔化 / 切割布防 / 区域着色 / 编辑图形 / 形状列表）的收口集中在下方
    /// 的 `tool != Tool::ShapeSelect` 分支；刀的跨工具保留是 DEBT-17 的独立口径，不在此列。
    pub fn set_tool(&mut self, tool: Tool) {
        // DEBT-17：换工具前先收敛失效的刀（源形状/音符可能已被删）
        self.sync_mould_knife();
        // DEBT-19：工具未变 → 镜像无需重建。
        //
        // `rebuild_pending_mirrors` 会 `remove_pending_all` 再逐项重算几何、重发 ID，
        // 而 owning 预览 / 选中域在几何变化时已有显式 rebuild 调用点。同工具重复
        // 选中（面板条目重复点击、工具栏按钮再点一次）不改变任何产物几何，重建纯属
        // 白付 O(待确认) 与一次选中态扰动。
        //
        // 其余收口照旧执行：重复选中同一工具仍应退出填充 / 圆弧 / 下拉等附属态。
        let tool_changed = self.current_tool() != tool;
        // DRAW-044：离开柔化模式必须收口，否则组合结果没有镜像、曲线看起来「消失」，
        // 且遗留的拖拽快照会污染撤销栈。未改过半径时还原提升前的曲线。
        if tool != Tool::ShapeSelect
            && let Some(mode) = self.shape_tools.round_corner.clone()
        {
            self.end_round_corners();
            if !mode.changed && mode.composed + 1 == self.editor_state.pending_composed.len() {
                self.undo_pending_compose();
            }
        }
        // DRAW-025：切到鼠标工具前的音符选区不再快照成刀（DEBT-17）。
        // 刀由右键「以选中内容为刀」显式设置，跨工具保留到消费 / 失效 / 换轨 / Esc。
        self.editor_state.set_tool(tool);
        if tool != Tool::Curve {
            self.editor_state.line_tool.fill_enabled = false;
            // 三点圆弧是曲线工具的落点模式：离开曲线工具即退出
            self.editor_state.line_tool.set_arc_mode(false);
        }
        // 形状工具：离开时关闭其填充桶（避免遗留影响其他工具）
        if tool != Tool::Shape {
            self.editor_state.shape_tool.fill_enabled = false;
        }
        // DRAW-048（口径 B）：鼠标工具的临时会话在切走时统一收口。
        //
        // 这些会话的按下 / 拖动分派排在工具分派之前（见 `interaction/pressed.rs`），
        // 所以「只隐藏预览、不结束会话」会让看不到的手柄 / 布防吞掉其他工具里的点击。
        // 新增鼠标工具会话时必须加进这一段，而不是只在各自的预览层做门控。
        if tool != Tool::ShapeSelect {
            // 切割布防（DRAW-025）：不结束会在曲线 / 画笔里继续切形状。
            self.end_slice_cut();
            // 区域着色（DRAW-032B）：遗留画笔会在别的工具里继续上色。
            self.shape_tools.area_paint = None;
            self.shape_tools.list_open = false;
            // 020「编辑图形」是鼠标工具的子状态：离开即退出，回来不会突然少掉选框。
            self.shape_tools.edit_armed = None;
            self.shape_tools.group_edit = None;
        }
        if tool_changed {
            // DRAW-049：Bake 放置态只属于形状工具；换工具即收口会话（pending 保留）。
            self.shape_tools.bake_placement = None;
            self.rebuild_pending_mirrors();
        }
    }

    /// 某绘制工具的**待确认预览**此刻是否可交互（选中 / 拖拽 / 会话）
    ///
    /// 规则：拥有者工具激活时照旧；**鼠标工具（`Tool::ShapeSelect`，图形选中）激活时也可见可交互**。
    /// 依据：待确认内容不再因切换工具被丢弃（见 `EditorState::set_tool`），若选择工具下不渲染，
    /// 用户看到的就是「切过去图案就消失、无从框选」。其余工具不渲染，避免浮层干扰音符编辑。
    ///
    /// ⚠️ 这是**可交互**判据，不是唯一的渲染判据：待确认组合（柔化 / 组合产物）在其他绘制
    /// 工具下仍以只读方式显示（DRAW-048，见 `root/view/egui_chrome/shape_preview.rs` 的
    /// `draws_pending_read_only`）——看得见不等于改得动。
    ///
    /// ⚠️ 仅用于**几何预览与命中**（曲线/形状/画刷方块/文本框），不含 √× 按钮——
    /// 按钮仍是拥有者工具专属（确认/取消属于绘制工具的操作，选择工具不越权）。
    pub fn pending_preview_visible(&self, owner: Tool) -> bool {
        let current = self.current_tool();
        current == owner || current == Tool::ShapeSelect
    }

    /// 设置颜料桶填充模式开关（曲线工具与形状工具共享同一开关）
    pub fn set_fill_enabled(&mut self, enabled: bool) {
        self.editor_state.line_tool.fill_enabled = enabled;
        self.editor_state.shape_tool.fill_enabled = enabled;
    }

    /// 颜料桶填充模式是否开启
    pub fn fill_enabled(&self) -> bool {
        self.editor_state.line_tool.fill_enabled
    }

    /// **全局绘制粗细**（层数，1-100；1 层 = 单 key 高的像素宽度）
    ///
    /// 单一权威：音符画悬浮工具条的「粗细」滑杆与画刷下拉展示/写入的都是它
    /// （物理载体是 `Editor::brush.thickness`——见 `BrushConfig` 的类型注释：
    /// 该字段已从"画刷私有"提升为**全局**，字段名保留以免大改对话框 / 历史载荷）。
    ///
    /// 消费点（改一处必须一起看）：
    /// 1. 画刷笔画：`brush/cells.rs` 逐层铺 key + 每层音轨分配；
    /// 2. 曲线 / 形状的**描边**：`line_tool::stroke_band` 沿**线段法线**把中心线加粗成带
    ///    （水平段纵向、竖直段横向、拐角补外角方格——不是"一律向上铺 key"，那对竖直段
    ///    等于拉长）；
    /// 3. **画布预览**：曲线 / 形状的描边预览直接画"√ 会生成的音符块"
    ///    （`Editor::shape_outline_blocks` / `Editor::path_outline_blocks`），
    ///    与第 2 点共用 `stroke_band` 的同一份几何——**不再**用"线宽 = 层数 × 单 key 高"
    ///    的居中描边（那与单侧向外的产物差半个带，竖直边在粗细 2 时差 2 倍）。
    pub fn draw_thickness(&self) -> u8 {
        self.brush.thickness.max(BrushConfig::MIN_THICKNESS)
    }

    /// 设置颜料桶填充的**切分档位**（`Some(x)` = x 分音符切分；`None` = 整块填充）
    ///
    /// 由主窗口「分音符填充」对话框确认后调用。变更会同步进路径历史
    /// （有待确认内容时），使 Ctrl+Z 可撤销。
    pub fn set_fill_division(&mut self, division: Option<u32>) {
        self.editor_state.line_tool.set_fill_division(division);
        self.mark_notes_changed();
    }

    /// 颜料桶填充的切分档位（`None` = 整块填充）
    pub fn fill_division(&self) -> Option<u32> {
        self.editor_state.line_tool.fill_division
    }

    /// 设置曲线/形状工具生成音符的**最小长度档位**（修改接口）
    ///
    /// `Some(x)` = 至少 x 分音符（`4·ppq/x` tick，`Some(0)` 归一为 `None`）；
    /// `None` = 不设下限。默认 128 分音符
    /// （[`lumino_editor_state::LineToolState::DEFAULT_MIN_NOTE_DIVISION`]）——竖直段音符
    /// 原本只有 1 tick，实机缩放下不足 1px。只影响之后生成的音符，不改写已有音符。
    pub fn set_min_note_division(&mut self, division: Option<u32>) {
        self.editor_state.line_tool.set_min_note_division(division);
        self.mark_notes_changed();
    }

    /// 生成音符的最小长度档位（`None` = 不设下限）
    pub fn min_note_division(&self) -> Option<u32> {
        self.editor_state.line_tool.min_note_division
    }

    /// 当前框选弹簧动画状态（UI 渲染层读取）。
    pub fn selection_box_anim(&self) -> Option<crate::SelectionBoxAnimState> {
        self.selection_ui.anim.get()
    }

    /// 获取当前工具
    pub fn current_tool(&self) -> Tool {
        self.editor_state.tool
    }

    /// 检查音符数据是否已变化
    pub fn notes_changed(&self) -> bool {
        self.notes_changed
    }

    /// 清除音符变化标志
    pub fn clear_notes_changed(&mut self) {
        self.notes_changed = false;
    }

    /// 重置编辑器内部状态到默认值（释放私有字段内存）
    ///
    /// 供 `clear_editor()` 调用，重置本模块私有的字段：
    /// - `notes_changed`：音符变更标志
    /// - `playback_position`：播放指示线位置
    /// - `playback_key_state.scan`：播放键色增量扫描状态（避免旧文档残留）
    pub fn reset_internal_state(&mut self) {
        self.notes_changed = false;
        self.playback_position = 0.0;
        self.playback_key_state.reset_scan();
        self.velocity_panel = crate::velocity::VelocityPanel::new();
    }

    /// 设置播放时键盘颜色指示是否启用
    pub fn set_playback_key_colors_enabled(&mut self, enabled: bool) {
        self.playback_key_state.set_enabled(enabled);
    }

    /// Domino（TAKABO SOFT）剪贴板互粘是否启用（默认关闭，仅 Windows 生效）
    ///
    /// 跨平台方法：非 Windows 下 Domino 路径整体被 `#[cfg(windows)]` 裁掉，该字段无处可读；
    /// 保留跨平台读写是为了让 `Root` 的设置同步链路三平台一致，避免 cfg 分叉。
    #[inline]
    pub fn domino_clipboard_enabled(&self) -> bool {
        self.domino_clipboard_enabled
    }

    /// 设置 Domino（TAKABO SOFT）剪贴板互粘是否启用（仅 Windows 生效）
    #[inline]
    pub fn set_domino_clipboard_enabled(&mut self, enabled: bool) {
        self.domino_clipboard_enabled = enabled;
    }
}
