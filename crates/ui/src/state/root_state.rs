use crate::titlebar::mode_toggle::AppMode;
use crate::toolbar::DotType;
use crate::toolbar::NotePrecision;

/// 模式切换按钮的弹簧物理动画状态
#[derive(Debug, Clone)]
pub struct ToggleAnimationState {
    /// 动画进度 (0.0 = Editor, 1.0 = Waterfall)
    pub position: f32,
    /// 速度（用于弹簧物理模拟）
    pub velocity: f32,
    /// 目标位置
    pub target: f32,
    /// 是否正在动画中
    pub active: bool,
    /// 上次更新时间（用于计算 dt）
    pub last_update: Option<std::time::Instant>,
}

impl Default for ToggleAnimationState {
    fn default() -> Self {
        Self {
            position: 0.0,
            velocity: 0.0,
            target: 0.0,
            active: false,
            last_update: None,
        }
    }
}

impl ToggleAnimationState {
    const STIFFNESS: f64 = 200.0;
    const DAMPING: f64 = 15.0;
    const VELOCITY_THRESHOLD: f64 = 0.001;
    const POSITION_THRESHOLD: f64 = 0.001;

    pub fn new() -> Self {
        Self::default()
    }

    /// 启动动画到目标位置
    pub fn animate_to(&mut self, target: f32) {
        self.target = target;
        if !self.active {
            self.active = true;
            self.last_update = Some(std::time::Instant::now());
        }
    }

    /// 更新弹簧物理模拟，返回是否仍在动画中
    pub fn update(&mut self) -> bool {
        if !self.active {
            return false;
        }

        let now = std::time::Instant::now();
        let dt = self
            .last_update
            .map(|t| t.elapsed().as_secs_f64())
            .unwrap_or(0.016);
        self.last_update = Some(now);

        let dt = dt.min(0.05);

        let displacement = (self.position - self.target) as f64;
        let spring_force = -Self::STIFFNESS * displacement;
        let damping_force = -Self::DAMPING * self.velocity as f64;
        let acceleration = spring_force + damping_force;

        self.velocity += (acceleration * dt) as f32;
        self.position += self.velocity * dt as f32;

        let at_target = ((self.position - self.target).abs() as f64) < Self::POSITION_THRESHOLD
            && (self.velocity.abs() as f64) < Self::VELOCITY_THRESHOLD;

        if at_target {
            self.position = self.target;
            self.velocity = 0.0;
            self.active = false;
            false
        } else {
            true
        }
    }
}

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

/// 工程设置对话框状态
#[derive(Debug, Clone)]
pub struct ProjectSettingsDialogState {
    pub is_open: bool,
    /// 项目名称
    pub title: String,
    /// BPM 速度 (字符串以便编辑)
    pub tempo: String,
    /// 版权信息
    pub copyright: String,
    /// 创建日期 (格式化后的字符串)
    pub created_display: String,
    /// 累计创作时间 (秒)
    pub total_editing_time_seconds: f64,
}

impl Default for ProjectSettingsDialogState {
    fn default() -> Self {
        Self {
            is_open: false,
            title: String::new(),
            tempo: "120".to_string(),
            copyright: String::new(),
            created_display: String::new(),
            total_editing_time_seconds: 0.0,
        }
    }
}

impl ProjectSettingsDialogState {
    pub fn new() -> Self {
        Self::default()
    }

    /// 格式化累计创作时间 (自适应单位)
    pub fn format_editing_time(&self) -> String {
        let total_seconds = self.total_editing_time_seconds;
        if total_seconds < 1.0 {
            return "不足 1 秒".to_string();
        }

        let days = (total_seconds / 86400.0) as u64;
        let hours = ((total_seconds % 86400.0) / 3600.0) as u64;
        let minutes = ((total_seconds % 3600.0) / 60.0) as u64;
        let seconds = (total_seconds % 60.0) as u64;

        if days > 0 {
            format!("{} 天 {} 小时 {} 分钟", days, hours, minutes)
        } else if hours > 0 {
            format!("{} 小时 {} 分钟", hours, minutes)
        } else if minutes > 0 {
            format!("{} 分钟 {} 秒", minutes, seconds)
        } else {
            format!("{} 秒", seconds)
        }
    }

    /// 解析 BPM 值 (20-10000)
    pub fn parse_tempo(&self) -> Option<f64> {
        let value = self.tempo.parse::<f64>().ok()?;
        if (20.0..=10000.0).contains(&value) {
            Some(value)
        } else {
            None
        }
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
    ProjectSettings,
    Settings,
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
}

impl Default for LoadConfirmDialogState {
    fn default() -> Self {
        Self {
            is_open: false,
            file_name: String::new(),
            file_path: String::new(),
            size_mb: 0.0,
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
    /// 工程设置对话框状态
    pub project_settings_dialog: ProjectSettingsDialogState,
    /// 精度设置
    pub note_precision: NotePrecision,
    /// 系统字体列表
    pub system_fonts: Vec<lumino_core::font_scanner::FontInfo>,
    /// 当前应用模式（编辑器/瀑布流）
    pub current_mode: AppMode,
    /// 模式切换按钮动画状态
    pub toggle_animation: ToggleAnimationState,
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
            project_settings_dialog: ProjectSettingsDialogState::new(),
            note_precision: NotePrecision::default(),
            system_fonts: lumino_core::font_scanner::scan_system_fonts(),
            current_mode: AppMode::default(),
            toggle_animation: ToggleAnimationState::new(),
        }
    }
}
