//! 帮助话题表（DRAW-038）
//!
//! 只存中文原文 key；英文翻译在 `lumino-core::i18n::catalog::help`。
//! 新增话题必须同时登记英文，否则 `catalog` 的 help 测试会失败。

use lumino_message::Tool;

/// 帮助分区
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpSection {
    /// 基础
    Basics,
    /// 编辑
    Editing,
    /// 绘制
    Drawing,
    /// 参考
    Reference,
}

/// tip 来源
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpTip {
    /// 工具按钮显示名（与 toolbar tooltip 同源）
    Tool(Tool),
    /// 独立短句
    Text(&'static str),
}

/// 一个帮助话题
#[derive(Debug, Clone, Copy)]
pub struct HelpTopic {
    /// 稳定 id（用于首次提示持久化）
    pub id: &'static str,
    /// 分区
    pub section: HelpSection,
    /// 标题（中文 key）
    pub title: &'static str,
    /// tip（工具名或短句）
    pub tip: HelpTip,
    /// 正文（中文 key，空行分段）
    pub body: &'static str,
    /// 搜索词（中英混合，便于两种语言命中）
    pub words: &'static [&'static str],
}

impl HelpSection {
    /// 全部分区（渲染顺序）
    pub const ALL: &'static [Self] = &[Self::Basics, Self::Editing, Self::Drawing, Self::Reference];
}

/// 初始话题清单（按已实现功能；后续功能追加到末尾）
pub static TOPICS: &[HelpTopic] = &[
    HelpTopic {
        id: "overview",
        section: HelpSection::Basics,
        title: "界面总览",
        tip: HelpTip::Text("主界面布局与各区域职责"),
        body: "主窗口由标题栏、左侧音轨栏、中间钢琴卷帘或工程走带、右侧栏与底部状态栏组成。\n\n钢琴卷帘用时间轴表示小节与拍，用纵向键盘表示音高；工程走带按音轨显示整首曲子的音符块。顶部工具栏放播放、工具与精度，底部状态栏显示当前操作提示。",
        words: &[
            "界面",
            "布局",
            "工具栏",
            "状态栏",
            "overview",
            "layout",
            "ui",
        ],
    },
    HelpTopic {
        id: "navigation",
        section: HelpSection::Basics,
        title: "缩放与滚动",
        tip: HelpTip::Text("滚轮缩放、拖拽平移与视图复位"),
        body: "鼠标滚轮缩放时间轴；Shift + 滚轮缩放音高轴。按住中键或空格拖拽可以平移视图。\n\n触摸板支持双指捏合：在标尺上捏合只缩放时间，在键盘区捏合只缩放音高，在网格区同时缩放两轴。右下角状态栏会显示当前缩放比例。",
        words: &["缩放", "滚动", "平移", "滚轮", "zoom", "scroll", "pan"],
    },
    HelpTopic {
        id: "notes",
        section: HelpSection::Editing,
        title: "音符编辑",
        tip: HelpTip::Tool(Tool::Pencil),
        body: "指针工具用于点选、拖动与框选音符；铅笔工具用于创建音符；橡皮擦用于删除。创建音符的默认长度与当前精度一致。\n\n拖动音符边缘可以改长度，拖动主体可以改位置；按住 Shift 可做约束操作。所有编辑都进入撤销历史。",
        words: &[
            "音符", "铅笔", "橡皮", "拖动", "note", "pencil", "eraser", "drag",
        ],
    },
    HelpTopic {
        id: "selection",
        section: HelpSection::Editing,
        title: "框选与多选",
        tip: HelpTip::Tool(Tool::Pointer),
        body: "在空白处拖出选框可框选音符；按住 Shift 点击可增减选中项。Y 向框选工具会自动覆盖整个音高范围，只按时间轴量化。\n\n右键点击未选中的音符会把它设为当前目标；右键点击已选中的成员则保持整个多选不变。",
        words: &[
            "框选",
            "多选",
            "选择",
            "selection",
            "marquee",
            "multi-select",
        ],
    },
    HelpTopic {
        id: "undo",
        section: HelpSection::Editing,
        title: "撤销与重做",
        tip: HelpTip::Text("Ctrl+Z / Ctrl+Y 与编辑拦截"),
        body: "Ctrl+Z 撤销，Ctrl+Y 或 Ctrl+Shift+Z 重做。一次连续拖动或一次形状提交会合并为一个撤销单元，不会拆成几十步。\n\n如果当前还有未完成的拖动、绘制或调大小，撤销会被暂时拦截并给出提示，先松开鼠标再撤销。",
        words: &["撤销", "重做", "历史", "undo", "redo", "history"],
    },
    HelpTopic {
        id: "precision",
        section: HelpSection::Editing,
        title: "吸附与精度",
        tip: HelpTip::Text("音符精度、框选网格与自定义精度"),
        body: "工具栏的精度下拉决定新建音符长度、拖动吸附与框选网格。常见档位从全音符到 128 分音符，也可以选择自定义。\n\n自定义精度支持三连音、符点与除数；修改 PPQ 时，非自定义精度会按新 PPQ 重新换算，自定义精度保持绝对 tick 不变。",
        words: &["吸附", "精度", "网格", "ppq", "snap", "precision", "grid"],
    },
    HelpTopic {
        id: "draw_toolbox",
        section: HelpSection::Drawing,
        title: "音符画工具条",
        tip: HelpTip::Text("曲线、形状、画刷、文字与填充的统一入口"),
        body: "音符画工具条浮在卷帘底部，默认可以拖动到任意位置。点击条目切换工具；再次点击已启用的条目会打开该工具的设置。\n\n右键菜单、颜料桶填充与文字参数面板都从当前工具状态读取，不需要先切到别的窗口。",
        words: &["音符画", "工具条", "绘制", "draw", "toolbox", "toolbar"],
    },
    HelpTopic {
        id: "curve",
        section: HelpSection::Drawing,
        title: "曲线工具",
        tip: HelpTip::Tool(Tool::Curve),
        body: "曲线工具通过锚点绘制折线或贝塞尔路径。拖拽生成路径后按 √ 确认，音符沿路径展开；按 Esc 取消。\n\n曲线可以开启颜料桶填充封闭区域。确认后的路径会转为普通音符参与编辑与撤销。",
        words: &["曲线", "路径", "贝塞尔", "curve", "path", "bezier"],
    },
    HelpTopic {
        id: "shape",
        section: HelpSection::Drawing,
        title: "形状工具",
        tip: HelpTip::Tool(Tool::Shape),
        body: "形状工具支持矩形、圆形与三角形。拖拽出外接框后按 √ 生成音符；形状下拉可切换当前图形。\n\n形状可叠加修饰链（凸起、爪机、扫弦、切分、延音等），这些设置保存在形状参数里，重新编辑后按新参数再生成。",
        words: &[
            "形状",
            "矩形",
            "圆形",
            "三角形",
            "shape",
            "rectangle",
            "circle",
            "triangle",
        ],
    },
    HelpTopic {
        id: "brush",
        section: HelpSection::Drawing,
        title: "画刷工具",
        tip: HelpTip::Tool(Tool::Brush),
        body: "画刷用于自由手绘音符带。线宽由全局「粗细」决定，沿笔画法线铺开多个音高行。\n\n画刷设置可以指定每层使用的音轨；确认后笔画转为普通音符，并可通过撤销恢复绘制前的状态。",
        words: &["画刷", "笔画", "手绘", "brush", "stroke", "freehand"],
    },
    HelpTopic {
        id: "text",
        section: HelpSection::Drawing,
        title: "文字工具",
        tip: HelpTip::Tool(Tool::Text),
        body: "文字工具把文字光栅化为音符。拖出文本框后输入文字，按 √ 确认；再次点击已确认的文字形状可以重新编辑。\n\n参数面板支持字体、字号、字重、斜体、行距、阈值与 grow；LegacyBox 保留按框填充的旧行为，Parametric 使用参数化排版。",
        words: &["文字", "字体", "字号", "text", "font", "size", "parametric"],
    },
    HelpTopic {
        id: "fill",
        section: HelpSection::Drawing,
        title: "分音符填充",
        tip: HelpTip::Text("按 x 分音符切分封闭区域"),
        body: "颜料桶对曲线或形状围出的封闭区域做填充。再次点击已启用的颜料桶会打开「分音符填充」面板。\n\n输入 x 表示按 x 分音符切分；留空或 0 表示整块填充。输入支持整数表达式，例如 8*2。",
        words: &["填充", "颜料桶", "分音符", "fill", "bucket", "division"],
    },
    HelpTopic {
        id: "shortcuts",
        section: HelpSection::Reference,
        title: "快捷键",
        tip: HelpTip::Text("裸字母切工具与常用组合键"),
        body: "裸字母切工具：V 指针、Y Y 向框选、P 铅笔、B/F 画刷、E 橡皮、C/L 曲线、S 形状、M 鼠标工具、X 文字。\n\nCtrl+S 保存、Ctrl+Z 撤销、Ctrl+Y 重做、Ctrl+Q 量化、空格播放/暂停。文本输入框聚焦时字母键不会切工具。",
        words: &["快捷键", "热键", "shortcut", "hotkey", "key"],
    },
    HelpTopic {
        id: "share",
        section: HelpSection::Reference,
        title: "形状分享",
        tip: HelpTip::Text("一行文本复制与粘贴形状"),
        body: "在已确认图形上右键选择「复制为分享文本」，会把形状编码成一行 LUMINO1 文本；可在另一台机器用「从分享文本粘贴」还原。\n\n分享文本以 beats 存储坐标，跨 PPQ 也能保持音乐位置；粘贴会重新分配形状 id 并落到当前音轨。",
        words: &["分享", "复制", "粘贴", "share", "copy", "paste", "lumin"],
    },
    HelpTopic {
        id: "expr",
        section: HelpSection::Reference,
        title: "数字表达式",
        tip: HelpTip::Text("数字框支持 960*4 这类整数表达式"),
        body: "数字框可以输入整数表达式，例如 960*4、(60+4)*16、8*2。求值使用整数精确路径，不会先转浮点再转回来。\n\n非法表达式或越界值会保留原输入并显示错误，不会静默改成 0；指数上限为 64，避免幂运算把界面卡死。",
        words: &["表达式", "数字框", "计算", "expression", "math", "960"],
    },
];
