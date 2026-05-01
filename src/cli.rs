use clap::{Parser, Subcommand};

/// Lumino 命令行接口
#[derive(Parser, Debug)]
#[command(name = "lumino-rs")]
#[command(author = "BuickMeow")]
#[command(version = "0.1.0")]
#[command(about = "Lumino 音乐编辑器 - 命令行测试模式")]
pub struct Cli {
    /// 启用测试模式
    #[arg(short = 't', long = "test")]
    pub test_mode: bool,

    /// MIDI 文件路径（测试模式下使用）
    #[arg(short = 'm', long = "midi", value_name = "PATH")]
    pub midi_path: Option<String>,

    /// 测试持续时间（秒），不指定则持续测试
    #[arg(long = "test-time", value_name = "SECONDS")]
    pub test_time: Option<u64>,

    /// 日志功能选项：memory-usage（每2秒输出各组件内存占用）
    #[arg(long = "log", value_name = "FEATURE")]
    pub log: Option<String>,

    /// 子命令
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// 音符渲染 FPS 测试
    #[command(name = "note-render-fps-test")]
    NoteRenderFpsTest {
        /// MIDI 文件路径
        #[arg(short = 'm', long = "midi", value_name = "PATH")]
        midi_path: String,

        /// 测试持续时间（秒），不指定则持续测试
        #[arg(long = "test-time", value_name = "SECONDS")]
        test_time: Option<u64>,
    },
}

impl Cli {
    /// 解析命令行参数
    pub fn parse_args() -> Self {
        Cli::parse()
    }

    /// 检查是否启用了测试模式
    pub fn is_test_mode(&self) -> bool {
        self.test_mode || matches!(self.command, Some(Commands::NoteRenderFpsTest { .. }))
    }

    /// 检查是否启用了 memory-usage 日志
    pub fn log_memory_usage(&self) -> bool {
        self.log.as_deref() == Some("memory-usage")
    }

    /// 获取测试配置
    pub fn get_test_config(&self) -> Option<TestConfig> {
        match &self.command {
            Some(Commands::NoteRenderFpsTest {
                midi_path,
                test_time,
            }) => Some(TestConfig {
                midi_path: midi_path.clone(),
                test_time: *test_time,
                test_type: TestType::NoteRenderFps,
            }),
            None if self.test_mode => Some(TestConfig {
                midi_path: self.midi_path.clone()?,
                test_time: self.test_time,
                test_type: TestType::NoteRenderFps,
            }),
            _ => None,
        }
    }
}

/// 测试类型
#[derive(Debug, Clone, Copy)]
pub enum TestType {
    /// 音符渲染 FPS 测试
    NoteRenderFps,
}

/// 测试配置
#[derive(Debug, Clone)]
pub struct TestConfig {
    /// MIDI 文件路径
    pub midi_path: String,
    /// 测试时间（秒），None 表示持续测试
    pub test_time: Option<u64>,
    /// 测试类型
    pub test_type: TestType,
}
