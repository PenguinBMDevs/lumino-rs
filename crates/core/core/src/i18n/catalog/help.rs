//! 文案目录分域：帮助系统（DRAW-038）
//!
//! 话题表只存中文 key；本文件提供英文。工具按钮名仍来自 `MainTranslations`，
//! 工具 tip 不重复登记。

/// 查本域英文译文；未登记返回 `None`。
pub fn lookup(zh: &str) -> Option<&'static str> {
    match zh {
        "基础" => Some("Basics"),
        "编辑" => Some("Editing"),
        "绘制" => Some("Drawing"),
        "参考" => Some("Reference"),

        "界面总览" => Some("Interface Overview"),
        "主界面布局与各区域职责" => Some("Main window layout and each area's role"),
        "主窗口由标题栏、左侧音轨栏、中间钢琴卷帘或工程走带、右侧栏与底部状态栏组成。\n\n钢琴卷帘用时间轴表示小节与拍，用纵向键盘表示音高；工程走带按音轨显示整首曲子的音符块。顶部工具栏放播放、工具与精度，底部状态栏显示当前操作提示。" => {
            Some(
                "The main window consists of the title bar, left track list, piano roll or arrangement view, right sidebar, and bottom status bar.\n\nThe piano roll uses a time axis for bars and beats and a vertical keyboard for pitch; the arrangement view shows note blocks per track. The top toolbar holds transport, tools, and precision; the status bar shows the current hint.",
            )
        }

        "缩放与滚动" => Some("Zoom and Scroll"),
        "滚轮缩放、拖拽平移与视图复位" => {
            Some("Wheel zoom, drag pan, and view reset")
        }
        "鼠标滚轮缩放时间轴；Shift + 滚轮缩放音高轴。按住中键或空格拖拽可以平移视图。\n\n触摸板支持双指捏合：在标尺上捏合只缩放时间，在键盘区捏合只缩放音高，在网格区同时缩放两轴。右下角状态栏会显示当前缩放比例。" => {
            Some(
                "Use the mouse wheel to zoom the time axis; hold Shift while wheeling to zoom the pitch axis. Hold the middle button or Space and drag to pan.\n\nTrackpads support pinch: over the ruler it zooms time only, over the keyboard it zooms pitch only, and over the grid it zooms both axes. The status bar shows the current zoom.",
            )
        }

        "音符编辑" => Some("Note Editing"),
        "指针工具用于点选、拖动与框选音符；铅笔工具用于创建音符；橡皮擦用于删除。创建音符的默认长度与当前精度一致。\n\n拖动音符边缘可以改长度，拖动主体可以改位置；按住 Shift 可做约束操作。所有编辑都进入撤销历史。" => {
            Some(
                "The pointer selects, drags, and marquees notes; the pencil creates notes; the eraser deletes them. New notes use the current precision as their default length.\n\nDrag a note edge to resize and its body to move; hold Shift for constrained edits. All edits enter undo history.",
            )
        }

        "框选与多选" => Some("Marquee and Multi-select"),
        "在空白处拖出选框可框选音符；按住 Shift 点击可增减选中项。Y 向框选工具会自动覆盖整个音高范围，只按时间轴量化。\n\n右键点击未选中的音符会把它设为当前目标；右键点击已选中的成员则保持整个多选不变。" => {
            Some(
                "Drag on empty space to marquee notes; Shift-click to add or remove selection. The Y-range tool covers all pitches and only quantizes time.\n\nRight-clicking an unselected note makes it the target; right-clicking a selected member keeps the whole multi-selection.",
            )
        }

        "撤销与重做" => Some("Undo and Redo"),
        "Ctrl+Z / Ctrl+Y 与编辑拦截" => Some("Ctrl+Z / Ctrl+Y and edit interception"),
        "Ctrl+Z 撤销，Ctrl+Y 或 Ctrl+Shift+Z 重做。一次连续拖动或一次形状提交会合并为一个撤销单元，不会拆成几十步。\n\n如果当前还有未完成的拖动、绘制或调大小，撤销会被暂时拦截并给出提示，先松开鼠标再撤销。" => {
            Some(
                "Ctrl+Z undoes; Ctrl+Y or Ctrl+Shift+Z redoes. A continuous drag or one shape commit is a single undo unit, not dozens of steps.\n\nIf a drag, draw, or resize is still in progress, undo is temporarily blocked with a hint; release the mouse first.",
            )
        }

        "吸附与精度" => Some("Snap and Precision"),
        "音符精度、框选网格与自定义精度" => {
            Some("Note precision, marquee grid, and custom precision")
        }
        "工具栏的精度下拉决定新建音符长度、拖动吸附与框选网格。常见档位从全音符到 128 分音符，也可以选择自定义。\n\n自定义精度支持三连音、附点与除数；修改 PPQ 时，非自定义精度会按新 PPQ 重新换算，自定义精度保持绝对 tick 不变。" => {
            Some(
                "The precision dropdown controls new-note length, drag snapping, and the marquee grid, from whole notes to 128th notes, plus Custom.\n\nCustom precision supports tuplets, dots, and divisors. On PPQ changes, preset precision is rescaled; Custom keeps its absolute tick value.",
            )
        }

        "音符画工具条" => Some("Draw Toolbox"),
        "曲线、形状、画刷、文字与填充的统一入口" => {
            Some("One entry for curve, shape, brush, text, and fill")
        }
        "音符画工具条浮在卷帘底部，默认可以拖动到任意位置。点击条目切换工具；再次点击已启用的条目会打开该工具的设置。\n\n右键菜单、颜料桶填充与文字参数面板都从当前工具状态读取，不需要先切到别的窗口。" => {
            Some(
                "The draw toolbox floats above the piano roll and can be dragged anywhere. Click an item to switch tools; click an enabled item again to open its settings.\n\nThe context menu, fill bucket, and text parameter panel read the current tool state; no extra window is needed.",
            )
        }

        "曲线工具" => Some("Curve Tool"),
        "曲线工具通过锚点绘制折线或贝塞尔路径。拖拽生成路径后按 √ 确认，音符沿路径展开；按 Esc 取消。\n\n曲线可以开启颜料桶填充封闭区域。确认后的路径会转为普通音符参与编辑与撤销。" => {
            Some(
                "The curve tool draws polylines or Bezier paths through anchors. Drag a path and press √ to confirm; notes spread along it. Press Esc to cancel.\n\nThe bucket can fill closed regions. A confirmed path becomes ordinary notes and participates in editing and undo.",
            )
        }

        "形状工具" => Some("Shape Tool"),
        "形状工具支持矩形、圆形与三角形。拖拽出外接框后按 √ 生成音符；形状下拉可切换当前图形。\n\n形状可叠加修饰链（凸起、爪机、扫弦、切分、延音等），这些设置保存在形状参数里，重新编辑后按新参数再生成。" => {
            Some(
                "The shape tool supports rectangles, circles, and triangles. Drag a bounding box and press √ to generate notes; the shape dropdown switches the current kind.\n\nShapes can carry modifier chains (tumours, claw, strum, chop, sustain, and more). Settings stay with the shape and regenerate notes when edited.",
            )
        }

        "画刷工具" => Some("Brush Tool"),
        "画刷用于自由手绘音符带。线宽由全局「粗细」决定，沿笔画法线铺开多个音高行。\n\n画刷设置可以指定每层使用的音轨；确认后笔画转为普通音符，并可通过撤销恢复绘制前的状态。" => {
            Some(
                "The brush draws freehand note bands. The global thickness sets the width in pitch rows along the stroke normal.\n\nBrush settings can assign each layer to a track. A confirmed stroke becomes ordinary notes and undo restores the pre-draw state.",
            )
        }

        "文字工具" => Some("Text Tool"),
        "文字工具把文字光栅化为音符。拖出文本框后输入文字，按 √ 确认；再次点击已确认的文字形状可以重新编辑。\n\n参数面板支持字体、字号、字重、斜体、行距、阈值与 grow；LegacyBox 保留按框填充的旧行为，Parametric 使用参数化排版。" => {
            Some(
                "The text tool rasterizes text into notes. Drag a text box, type, and press √; click a confirmed text shape to edit it again.\n\nThe parameter panel supports font, size, weight, italic, leading, threshold, and grow. LegacyBox keeps the old box-fill behavior; Parametric uses parameterized layout.",
            )
        }

        "分音符填充" => Some("Note-Division Fill"),
        "按 x 分音符切分封闭区域" => Some("Split a closed region by x-note divisions"),
        "颜料桶对曲线或形状围出的封闭区域做填充。再次点击已启用的颜料桶会打开「分音符填充」面板。\n\n输入 x 表示按 x 分音符切分；留空或 0 表示整块填充。输入支持整数表达式，例如 8*2。" => {
            Some(
                "The bucket fills regions enclosed by curves or shapes. Click an enabled bucket again to open the fill panel.\n\nEnter x to split by x-note divisions; empty or 0 fills the whole region. Integer expressions such as 8*2 are accepted.",
            )
        }

        "快捷键" => Some("Keyboard Shortcuts"),
        "裸字母切工具与常用组合键" => {
            Some("Bare-letter tool switching and common shortcuts")
        }
        "裸字母切工具：V 指针、Y Y 向框选、P 铅笔、B/F 画刷、E 橡皮、C/L 曲线、S 形状、M 鼠标工具、X 文字。\n\nCtrl+S 保存、Ctrl+Z 撤销、Ctrl+Y 重做、Ctrl+Q 量化、空格播放/暂停。文本输入框聚焦时字母键不会切工具。" => {
            Some(
                "Bare letters switch tools: V pointer, Y Y-range select, P pencil, B/F brush, E eraser, C/L curve, S shape, M mouse tool, X text.\n\nCtrl+S saves, Ctrl+Z undoes, Ctrl+Y redoes, Ctrl+Q quantizes, Space toggles playback. Letter shortcuts are ignored while a text input is focused.",
            )
        }

        "形状分享" => Some("Shape Sharing"),
        "一行文本复制与粘贴形状" => Some("Copy and paste shapes as one line of text"),
        "在已确认图形上右键选择「复制为分享文本」，会把形状编码成一行 LUMINO1 文本；可在另一台机器用「从分享文本粘贴」还原。\n\n分享文本以 beats 存储坐标，跨 PPQ 也能保持音乐位置；粘贴会重新分配形状 id 并落到当前音轨。" => {
            Some(
                "Right-click a confirmed shape and choose Copy as Share Text to encode it as one LUMINO1 line; use Paste Share Text on another machine to restore it.\n\nCoordinates are stored in beats, so musical positions survive PPQ changes. Pasting assigns new shape ids and uses the current track.",
            )
        }

        "数字表达式" => Some("Numeric Expressions"),
        "数字框支持 960*4 这类整数表达式" => {
            Some("Numeric fields accept integer expressions such as 960*4")
        }
        "数字框可以输入整数表达式，例如 960*4、(60+4)*16、8*2。求值使用整数精确路径，不会先转浮点再转回来。\n\n非法表达式或越界值会保留原输入并显示错误，不会静默改成 0；指数上限为 64，避免幂运算把界面卡死。" => {
            Some(
                "Numeric fields accept integer expressions such as 960*4, (60+4)*16, or 8*2. Evaluation uses an exact integer path, never a round-trip through floating point.\n\nInvalid or out-of-range input keeps the original text and shows an error instead of silently becoming 0. The exponent limit is 64 so powers cannot freeze the UI.",
            )
        }

        // 无独立按钮的工具名（帮助/诊断）
        "钢笔工具" => Some("Pen Tool"),
        "切割工具" => Some("Razor Tool"),
        "绘制橡皮" => Some("Draw Eraser"),
        _ => None,
    }
}
