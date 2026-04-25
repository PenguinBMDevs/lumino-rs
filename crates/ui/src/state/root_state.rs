use crate::toolbar::DotType;
use crate::toolbar::NotePrecision;

/// 自定义精度对话框状态
#[derive(Debug, Clone)]
pub struct CustomPrecisionDialogState {
    pub is_open: bool,
    pub tuplet_count: String,
    pub note_value: String,
    pub tuplet_type: crate::toolbar::TupletType,
    pub dot_type: DotType,
    pub divisor: String,
}

impl Default for CustomPrecisionDialogState {
    fn default() -> Self {
        Self {
            is_open: false,
            tuplet_count: "3".to_string(),
            note_value: "4".to_string(),
            tuplet_type: crate::toolbar::TupletType::Triplet,
            dot_type: DotType::None,
            divisor: "2".to_string(),
        }
    }
}

impl CustomPrecisionDialogState {
    pub fn new() -> Self {
        Self::default()
    }

    /// 计算自定义精度对应的 tick 数
    pub fn calculate_ticks(&self, ppq: u32) -> Option<f32> {
        let numerator = self.tuplet_count.parse::<f32>().ok()?;
        let denominator = self.note_value.parse::<f32>().ok()?;
        let divisor = self.divisor.parse::<f32>().ok()?;

        if denominator == 0.0 || divisor == 0.0 {
            return None;
        }

        // 计算基础 tick 数
        let base_ticks = (ppq as f32) * 4.0 * numerator / denominator;

        // 应用除数
        let ticks = base_ticks / divisor;

        Some(ticks)
    }
}

/// 对话框类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DialogType {
    #[default]
    None,
    CustomPrecision,
    Collaboration,
    LoadConfirm,
}

/// 协作视图状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CollaborationViewState {
    #[default]
    Connect, // 连接服务器界面
    Connecting,  // 正在连接中
    RoomActions, // 创建/加入房间界面
    InRoom,      // 在房间内界面
}

/// 加载确认对话框状态
#[derive(Debug, Clone)]
pub struct LoadConfirmDialogState {
    pub is_open: bool,
    pub file_name: String,
    pub file_path: String,
    pub size_mb: f64,
    /// 是否启用内存优化
    pub skip_memory_manager: bool,
}

impl Default for LoadConfirmDialogState {
    fn default() -> Self {
        Self {
            is_open: false,
            file_name: String::new(),
            file_path: String::new(),
            size_mb: 0.0,
            skip_memory_manager: true, // 默认开启
        }
    }
}

/// 协作对话框状态
#[derive(Debug, Clone)]
pub struct CollaborationDialogState {
    pub is_open: bool,
    /// 服务器地址
    pub server_host: String,
    /// 服务器端口
    pub server_port: String,
    /// 用户名
    pub username: String,
    /// 房间名称（创建房间用）
    pub room_name: String,
    /// 邀请码（加入房间用）
    pub invite_code: String,
    /// 当前视图状态
    pub view_state: CollaborationViewState,
    /// 连接状态
    pub connection_status: String,
}

impl CollaborationDialogState {
    pub fn new() -> Self {
        Self {
            is_open: false,
            server_host: "localhost".to_string(),
            server_port: "3000".to_string(),
            username: "用户".to_string(),
            room_name: "我的房间".to_string(),
            invite_code: String::new(),
            view_state: CollaborationViewState::Connect,
            connection_status: String::new(),
        }
    }

    pub fn reset(&mut self) {
        self.is_open = false;
        self.view_state = CollaborationViewState::Connect;
        self.connection_status.clear();
    }
}

/// Root 组件的状态
pub struct RootState {
    /// 是否有菜单/下拉框打开（打开时不渲染预览音符）
    pub is_menu_open: bool,
    /// 对话框结果（用于独立窗口模式）
    pub dialog_result: Option<crate::host::DialogResult>,
    /// 是否是对话框窗口（用于自定义精度对话框等）
    pub is_dialog_window: bool,
    /// 对话框类型
    pub dialog_type: DialogType,
    /// 自定义精度对话框状态
    pub custom_precision_dialog: CustomPrecisionDialogState,
    /// 加载确认对话框状态
    pub load_confirm_dialog: LoadConfirmDialogState,
    /// 协作对话框状态
    pub collaboration_dialog: CollaborationDialogState,
    /// 精度设置
    pub note_precision: NotePrecision,
    /// 系统字体列表
    pub system_fonts: Vec<lumino_core::font_scanner::FontInfo>,
}

impl RootState {
    pub fn new() -> Self {
        Self {
            is_menu_open: false,
            dialog_result: None,
            is_dialog_window: false,
            dialog_type: DialogType::None,
            custom_precision_dialog: CustomPrecisionDialogState::new(),
            load_confirm_dialog: LoadConfirmDialogState::default(),
            collaboration_dialog: CollaborationDialogState::new(),
            note_precision: NotePrecision::default(),
            system_fonts: lumino_core::font_scanner::scan_system_fonts(),
        }
    }
}
