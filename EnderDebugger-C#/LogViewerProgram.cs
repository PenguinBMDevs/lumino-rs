using System;
using System.IO;
using System.Threading;
using System.Diagnostics;
using System.Text.Json;
using System.Text.RegularExpressions;
using System.Collections.Generic;
using System.Linq;

namespace EnderDebugger
{
    /// <summary>
    /// 日志查看器 - 独立程序入口
    /// 这是从LuminoLogViewer合并过来的功能
    /// </summary>
    public sealed class LogViewerProgram
    {
        private static string _logFilePath = "";
        private static long _lastPosition = 0;
        private static readonly object _consoleLock = new object();
        private static HashSet<string> _enabledLevels = new HashSet<string> { "DEBUG", "INFO", "WARN", "ERROR", "FATAL" };
        private static string? _searchTerm = null;
        private static bool _followFile = true;
        private static int _maxLines = 1000;
        private static bool _showTimestamp = true;

        /// <summary>
        /// 启动日志查看器
        /// </summary>
        [STAThread]
        public static void Main(string[] args)
        {
            PrintHeader();

            // 解析命令行参数
            ParseCommandLineArgs(args);

            // 启用VT100颜色支持
            Console.OutputEncoding = System.Text.Encoding.UTF8;

            // 获取日志文件路径
            string? projectRoot = FindProjectRoot();
            if (projectRoot == null)
            {
                projectRoot = Directory.GetCurrentDirectory() ?? ".";
            }

            var logDir = Path.Combine(projectRoot, "EnderDebugger", "Logs");
            Directory.CreateDirectory(logDir);
            _logFilePath = Path.Combine(logDir, "LuminoLogViewer.log");

            if (!File.Exists(_logFilePath))
            {
                File.WriteAllText(_logFilePath, "");
            }

            PrintStatus("正在初始化日志监听器...");
            PrintConfiguration();

            // 读取现有日志内容
            ReadExistingLogs();

            // 设置文件监听器 - 已删除自动滚动功能
            // SetupFileWatcher();
            PrintStatus("文件监控已禁用（移除自动滚动）");

            PrintStatus("日志查看器已启动 (EnderDebugger集成版本 - 无自动滚动)");
            PrintHelp();

            // 保持程序运行
            Console.CancelKeyPress += (sender, e) =>
            {
                PrintStatus("正在退出日志查看器...");
                Console.ResetColor();
                Environment.Exit(0);
            };

            while (true)
            {
                Thread.Sleep(1000);
            }
        }

        /// <summary>
        /// 解析命令行参数
        /// </summary>
        private static void ParseCommandLineArgs(string[] args)
        {
            for (int i = 0; i < args.Length; i++)
            {
                switch (args[i].ToLower())
                {
                    case "--levels":
                    case "-l":
                        if (i + 1 < args.Length)
                        {
                            var levels = args[i + 1].Split(',');
                            _enabledLevels = levels.Select(l => l.Trim().ToUpper()).ToHashSet();
                            i++; // 跳过下一个参数
                        }
                        break;

                    case "--search":
                    case "-s":
                        if (i + 1 < args.Length)
                        {
                            _searchTerm = args[i + 1];
                            i++;
                        }
                        break;

                    case "--max-lines":
                    case "-n":
                        if (i + 1 < args.Length && int.TryParse(args[i + 1], out var lines))
                        {
                            _maxLines = Math.Max(1, lines);
                            i++;
                        }
                        break;

                    case "--no-follow":
                    case "-f":
                        _followFile = false;
                        break;

                    case "--no-timestamp":
                    case "-t":
                        _showTimestamp = false;
                        break;

                    case "--help":
                    case "-h":
                        PrintFullHelp();
                        Environment.Exit(0);
                        break;
                }
            }
        }

        /// <summary>
        /// 打印配置信息
        /// </summary>
        private static void PrintConfiguration()
        {
            Console.ForegroundColor = ConsoleColor.Yellow;
            Console.WriteLine("当前配置:");
            Console.WriteLine($"  📁 日志文件: {_logFilePath}");
            Console.WriteLine($"  🏷️  启用级别: {string.Join(", ", _enabledLevels.OrderBy(l => GetLevelPriority(l)))}");
            if (!string.IsNullOrEmpty(_searchTerm))
                Console.WriteLine($"  🔍 搜索词: {_searchTerm}");
            Console.WriteLine($"  📄 最大行数: {_maxLines}");
            Console.WriteLine($"  👁️  跟踪文件: {(_followFile ? "是" : "否")}");
            Console.WriteLine($"  ⏰ 显示时间: {(_showTimestamp ? "是" : "否")}");
            Console.WriteLine();
            Console.ResetColor();
        }

        /// <summary>
        /// 获取日志级别优先级
        /// </summary>
        private static int GetLevelPriority(string level)
        {
            return level switch
            {
                "DEBUG" => 0,
                "INFO" => 1,
                "WARN" => 2,
                "ERROR" => 3,
                "FATAL" => 4,
                _ => 5
            };
        }

        /// <summary>
        /// 查找项目根目录
        /// </summary>
        private static string? FindProjectRoot()
        {
            string? currentDir = Directory.GetCurrentDirectory();

            // 向上查找包含解决方案文件的目录
            DirectoryInfo? dir = currentDir != null ? new DirectoryInfo(currentDir) : null;
            while (dir != null)
            {
                if (File.Exists(Path.Combine(dir.FullName, "Lumino.sln")))
                {
                    return dir.FullName;
                }
                dir = dir.Parent;
            }

            // 如果找不到，返回当前目录
            return currentDir;
        }

        private static void ReadExistingLogs()
        {
            try
            {
                using (var stream = new FileStream(_logFilePath, FileMode.Open, FileAccess.Read, FileShare.ReadWrite))
                using (var reader = new StreamReader(stream))
                {
                    var lines = new List<string>();
                    string? line;
                    while ((line = reader.ReadLine()) != null)
                    {
                        lines.Add(line);
                        if (lines.Count > _maxLines)
                        {
                            lines.RemoveAt(0);
                        }
                    }

                    foreach (var logLine in lines)
                    {
                        ProcessLogLine(logLine);
                    }

                    _lastPosition = stream.Position;
                }
            }
            catch (Exception ex)
            {
                PrintError($"读取现有日志时出错: {ex.Message}");
            }
        }

        private static void ProcessLogLine(string line)
        {
            if (string.IsNullOrWhiteSpace(line))
                return;

            lock (_consoleLock)
            {
                try
                {
                    // 尝试解析JSON格式
                    if (line.StartsWith("{"))
                    {
                        var logEntry = JsonSerializer.Deserialize<LogViewerEntry>(line);
                        if (logEntry != null)
                        {
                            if (ShouldDisplayLog(logEntry.Level, logEntry.Message))
                            {
                                PrintJsonLog(logEntry);
                            }
                            return;
                        }
                    }

                    // 尝试解析新的日志格式 [HH:mm:ss.fff] [LEVEL] [SOURCE] [COMPONENT] Message
                    var newFormat = ParseNewFormat(line);
                    if (newFormat != null)
                    {
                        if (ShouldDisplayLog(newFormat.Level, newFormat.Message))
                        {
                            PrintFormattedLog(newFormat);
                        }
                        return;
                    }

                    // 尝试解析旧的日志格式 [EnderDebugger][DATETIME][SOURCE][COMPONENT]Message
                    var oldFormat = ParseOldFormat(line);
                    if (oldFormat != null)
                    {
                        if (ShouldDisplayLog(oldFormat.Level, oldFormat.Message))
                        {
                            PrintFormattedLog(oldFormat);
                        }
                        return;
                    }

                    // 如果都解析失败，且没有搜索条件或匹配搜索词，则输出原始行
                    if (string.IsNullOrEmpty(_searchTerm) || line.Contains(_searchTerm, StringComparison.OrdinalIgnoreCase))
                    {
                        Console.WriteLine(line);
                    }
                }
                catch
                {
                    // 如果解析失败，直接输出原始行
                    if (string.IsNullOrEmpty(_searchTerm) || line.Contains(_searchTerm, StringComparison.OrdinalIgnoreCase))
                    {
                        Console.WriteLine(line);
                    }
                }
            }
        }

        /// <summary>
        /// 判断是否应该显示日志
        /// </summary>
        private static bool ShouldDisplayLog(string level, string message)
        {
            // 检查日志级别
            if (!_enabledLevels.Contains(level.Trim().ToUpper()))
                return false;

            // 检查搜索词
            if (!string.IsNullOrEmpty(_searchTerm))
            {
                return message.Contains(_searchTerm, StringComparison.OrdinalIgnoreCase) ||
                       level.Contains(_searchTerm, StringComparison.OrdinalIgnoreCase);
            }

            return true;
        }

        /// <summary>
        /// 解析新的日志格式 [HH:mm:ss.fff] [LEVEL] [SOURCE] [COMPONENT] Message
        /// </summary>
        private static LogData? ParseNewFormat(string line)
        {
            var pattern = @"\[(\d{2}:\d{2}:\d{2}\.\d{3})\]\s*\[(\w+)\]\s*\[([^\]]+)\]\s*\[([^\]]+)\]\s*(.*)";
            var match = Regex.Match(line, pattern);

            if (match.Success)
            {
                return new LogData
                {
                    Timestamp = match.Groups[1].Value,
                    Level = match.Groups[2].Value.Trim(),
                    Source = match.Groups[3].Value.Trim(),
                    Component = match.Groups[4].Value.Trim(),
                    Message = match.Groups[5].Value.Trim()
                };
            }

            return null;
        }

        /// <summary>
        /// 解析旧的日志格式 [EnderDebugger][DATETIME][SOURCE][COMPONENT]Message
        /// </summary>
        private static LogData? ParseOldFormat(string line)
        {
            var pattern = @"\[EnderDebugger\]\[([^\]]+)\]\[([^\]]+)\]\[([^\]]+)\]\s*(.*)";
            var match = Regex.Match(line, pattern);

            if (match.Success)
            {
                // 解析日期时间
                string dateTimeStr = match.Groups[1].Value;
                string source = match.Groups[2].Value.Trim();
                string component = match.Groups[3].Value.Trim();
                string message = match.Groups[4].Value.Trim();

                // 尝试从日期时间中提取时间部分
                var timeMatch = Regex.Match(dateTimeStr, @"(\d{2}:\d{2}:\d{2}\.\d{3})");
                string timestamp = timeMatch.Success ? timeMatch.Groups[1].Value : "00:00:00.000";

                return new LogData
                {
                    Timestamp = timestamp,
                    Level = "INFO", // 旧格式没有级别信息，默认为INFO
                    Source = source,
                    Component = component,
                    Message = message
                };
            }

            return null;
        }

        /// <summary>
        /// 打印JSON格式的日志
        /// </summary>
        private static void PrintJsonLog(LogViewerEntry logEntry)
        {
            string timestamp = _showTimestamp ? logEntry.Timestamp.ToString("HH:mm:ss.fff") : "";
            string levelText = GetLevelText(logEntry.Level);
            string levelColor = GetLevelColor(logEntry.Level);
            string resetColor = "\u001b[0m";

            if (_showTimestamp)
                Console.WriteLine($"{levelColor}[{timestamp}] [{levelText}] [{logEntry.Component}] [LogViewer] {logEntry.Message}{resetColor}");
            else
                Console.WriteLine($"{levelColor}[{levelText}] [{logEntry.Component}] [LogViewer] {logEntry.Message}{resetColor}");
        }

        /// <summary>
        /// 打印格式化后的日志
        /// </summary>
        private static void PrintFormattedLog(LogData logData)
        {
            string levelText = GetLevelText(logData.Level);
            string levelColor = GetLevelColor(logData.Level);
            string sourceColor = "\u001b[36m"; // 青色显示SOURCE
            string componentColor = "\u001b[35m"; // 紫色显示COMPONENT
            string resetColor = "\u001b[0m";

            // 按列对齐格式化输出
            if (_showTimestamp)
                Console.WriteLine($"{levelColor}[{logData.Timestamp}] [{levelText}] {sourceColor}[{logData.Source}] {componentColor}[{logData.Component}] {resetColor}{logData.Message}");
            else
                Console.WriteLine($"{levelColor}[{levelText}] {sourceColor}[{logData.Source}] {componentColor}[{logData.Component}] {resetColor}{logData.Message}");
        }

        /// <summary>
        /// 获取日志级别文本
        /// </summary>
        private static string GetLevelText(string level)
        {
            switch (level.Trim().ToUpper())
            {
                case "DEBUG":
                    return "DEBUG";
                case "INFO":
                    return "INFO ";
                case "WARN":
                case "WARNING":
                    return "WARN ";
                case "ERROR":
                    return "ERROR";
                case "FATAL":
                    return "FATAL";
                default:
                    return "UNKNOWN";
            }
        }

        /// <summary>
        /// 获取日志级别对应的颜色标识
        /// </summary>
        private static string GetLevelColor(string level)
        {
            switch (level.Trim().ToUpper())
            {
                case "DEBUG":
                    return "\u001b[38;5;14m"; // 亮青色
                case "INFO":
                    return "\u001b[38;5;10m"; // 亮绿色
                case "WARN":
                case "WARNING":
                    return "\u001b[38;5;11m"; // 亮黄色
                case "ERROR":
                    return "\u001b[38;5;9m";  // 亮红色
                case "FATAL":
                    return "\u001b[38;5;13m"; // 亮紫色
                default:
                    return "\u001b[38;5;7m";  // 亮灰色
            }
        }

        /// <summary>
        /// 打印标题
        /// </summary>
        private static void PrintHeader()
        {
            Console.Clear();
            Console.ForegroundColor = ConsoleColor.Cyan;
            Console.WriteLine("╔══════════════════════════════════════════════════════════════╗");
            Console.WriteLine("║                     日志查看器 v3.0                           ║");
            Console.WriteLine("║                   (EnderDebugger集成版本)                     ║");
            Console.WriteLine("║                                                              ║");
            Console.WriteLine("║  ✨ 支持多种日志格式 ✨                                       ║");
            Console.WriteLine("║  🎨 彩色输出显示 🏷️ 级别过滤 🔍 搜索 📝 实时监控                ║");
            Console.WriteLine("╚══════════════════════════════════════════════════════════════╝");
            Console.ResetColor();
            Console.WriteLine();
        }

        /// <summary>
        /// 打印帮助信息
        /// </summary>
        private static void PrintHelp()
        {
            Console.ForegroundColor = ConsoleColor.Gray;
            Console.WriteLine("快捷键:");
            Console.WriteLine("  Ctrl+C    退出程序");
            Console.WriteLine();
            Console.WriteLine("命令行选项:");
            Console.WriteLine("  --levels <levels>    指定日志级别 (DEBUG,INFO,WARN,ERROR,FATAL)");
            Console.WriteLine("  --search <term>      搜索日志内容");
            Console.WriteLine("  --max-lines <n>      最大显示行数");
            Console.WriteLine("  --no-follow          不跟踪文件变化");
            Console.WriteLine("  --no-timestamp       不显示时间戳");
            Console.WriteLine("  --help               显示帮助");
            Console.WriteLine();
            Console.ResetColor();
        }

        /// <summary>
        /// 打印完整帮助
        /// </summary>
        private static void PrintFullHelp()
        {
            Console.WriteLine();
            Console.ForegroundColor = ConsoleColor.Cyan;
            Console.WriteLine("日志查看器 - 完整帮助 (EnderDebugger集成版本)");
            Console.WriteLine("==========================================");
            Console.ResetColor();
            PrintHelp();

            Console.WriteLine("示例用法:");
            Console.ForegroundColor = ConsoleColor.Yellow;
            Console.WriteLine("  LogViewerProgram.exe");
            Console.WriteLine("  LogViewerProgram.exe --levels DEBUG,INFO --search \"error\"");
            Console.WriteLine("  LogViewerProgram.exe --max-lines 500 --no-follow");
            Console.ResetColor();
            Console.WriteLine();
        }

        /// <summary>
        /// 打印状态信息
        /// </summary>
        private static void PrintStatus(string message)
        {
            Console.ForegroundColor = ConsoleColor.Green;
            if (_showTimestamp)
                Console.WriteLine($"[{DateTime.Now:HH:mm:ss}] [INFO] {message}");
            else
                Console.WriteLine($"[INFO] {message}");
            Console.ResetColor();
        }

        /// <summary>
        /// 打印错误信息
        /// </summary>
        private static void PrintError(string message)
        {
            Console.ForegroundColor = ConsoleColor.Red;
            if (_showTimestamp)
                Console.WriteLine($"[{DateTime.Now:HH:mm:ss}] [ERROR] {message}");
            else
                Console.WriteLine($"[ERROR] {message}");
            Console.ResetColor();
        }

        private class LogViewerEntry
        {
            public string Level { get; set; } = "";
            public string Component { get; set; } = "";
            public string Message { get; set; } = "";
            public DateTime Timestamp { get; set; }
        }

        /// <summary>
        /// 解析后的日志数据
        /// </summary>
        private class LogData
        {
            public string Timestamp { get; set; } = "";
            public string Level { get; set; } = "";
            public string Source { get; set; } = "";
            public string Component { get; set; } = "";
            public string Message { get; set; } = "";
        }
    }
}
