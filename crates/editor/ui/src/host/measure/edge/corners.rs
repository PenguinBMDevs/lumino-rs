//! 待检角落清单：**对话框类型 × 视图状态**
//!
//! # 为什么需要状态维度
//!
//! 只扫「默认状态」会漏检，而且漏得很隐蔽——两次真实教训：
//!
//! 1. 设置面板只扫第 0 页时报告 17/17 干净，实际音频页/界面页/调色板页有
//!    11 处控件被压窄（一次验收返工）；
//! 2. 视频导出对话框默认 `overlay = None`，渲染的是「准备中…」占位——
//!    真正的 `Exporting`/`Finalizing`/`Completed`/`Error` 覆盖层内容
//!    **一次都没被扫过**，量出的尺寸约束与用户看到的界面无关。
//!
//! 因此尺子的遍历维度是 `类型 × 状态`，新增一类必须能说清「这个状态渲染的
//! 控件与默认状态不同」，否则只是给报告凑数。
//!
//! # 模块结构
//!
//! | 文件 | 职责 |
//! |------|------|
//! | `corners`（本文件） | 角落清单（类型 × 状态表）、全量扫描、违规路径定位 |
//! | `corners/states` | 多状态角落的**数据构造**（云浏览、找回音轨） |
//! | `corners/tests` | 自检：每个声明出来的状态都必须真的改变布局 |

use iced_core::Size;
use iced_core::layout::Node;

use crate::state::root_state::{CollaborationViewState, DialogType, VideoExportOverlayState};
use crate::{Renderer, Root};

use super::super::dialog_sizes::{ALL_DIALOGS, WORK_AREA};
use super::{decide_dialog_window_size, edge_report};

use states::{cloud_browser_filled, cloud_browser_save_mode, recover_track_filled};

mod states;
mod tests;

/// 把 `Root` 置为某个视图状态（`Box<dyn Fn>` 以容纳带参数的闭包）
type Setup = Box<dyn Fn(&mut Root)>;

/// 装箱一个状态设置函数
fn setup<F: Fn(&mut Root) + 'static>(mutate: F) -> Setup {
    Box::new(mutate)
}

/// 一个待检角落
pub(super) struct Case {
    pub(super) name: String,
    pub(super) window_size: Size,
    pub(super) node: Option<Node>,
    pub(super) violations: Vec<super::Violation>,
}

/// 扫一遍所有角落：每个对话框类型 × 它需要覆盖的视图状态
pub(super) fn scan_all(
    renderer: &Renderer,
    ui_config: &lumino_core::storage::config::UiConfig,
) -> Vec<Case> {
    let mut results = Vec::new();

    for dialog_type in ALL_DIALOGS {
        for (suffix, apply) in dialog_states(dialog_type) {
            let mut root = Root::new_dialog_with_config(&ui_config.theme, dialog_type, ui_config);
            apply(&mut root);

            let window_size = decide_dialog_window_size(&root, renderer, dialog_type, WORK_AREA);
            let (node, violations) = edge_report(&root, renderer, dialog_type);
            results.push(Case {
                name: format!("{dialog_type:?}{suffix}"),
                window_size,
                node,
                violations,
            });
        }
    }

    results
}

/// 按名字构造某个角落的 `Root`（诊断与自检共用）
///
/// `suffix` 是状态标签（空串 = 默认状态）；找不到时返回 `None`。
pub(super) fn root_for(
    dialog_type: DialogType,
    suffix: &str,
    ui_config: &lumino_core::storage::config::UiConfig,
) -> Option<Root> {
    let (_, apply) = dialog_states(dialog_type)
        .into_iter()
        .find(|(label, _)| label == suffix)?;

    let mut root = Root::new_dialog_with_config(&ui_config.theme, dialog_type, ui_config);
    apply(&mut root);

    Some(root)
}

/// 某个对话框类型需要扫描的状态清单（至少一项；后缀为空 = 默认状态）
fn dialog_states(dialog_type: DialogType) -> Vec<(String, Setup)> {
    match dialog_type {
        // ── 设置面板：10 个菜单页，各页控件完全不同 ──
        DialogType::Settings => super::SETTINGS_PAGES
            .iter()
            .map(|&page| {
                (
                    format!("[页 {page}]"),
                    setup(move |root: &mut Root| {
                        root.settings.selected_menu_index = page;
                    }),
                )
            })
            .collect(),

        // ── 协作：4 套互不相同的视图 ──
        DialogType::Collaboration => vec![
            (
                "[连接服务器]".to_string(),
                setup(|root: &mut Root| {
                    root.state.collaboration_dialog.view_state = CollaborationViewState::Connect;
                }),
            ),
            (
                "[连接中]".to_string(),
                setup(|root: &mut Root| {
                    root.state.collaboration_dialog.view_state = CollaborationViewState::Connecting;
                }),
            ),
            (
                "[创建加入]".to_string(),
                setup(|root: &mut Root| {
                    root.state.collaboration_dialog.view_state =
                        CollaborationViewState::RoomActions;
                }),
            ),
            (
                "[房间内]".to_string(),
                setup(|root: &mut Root| {
                    let state = &mut root.state.collaboration_dialog;
                    state.view_state = CollaborationViewState::InRoom;
                    state.room_name = "钢琴卷帘协作房间".to_string();
                    state.invite_code = "LUMI-8F3K-2Q9Z".to_string();
                    state.connection_status = "已连接到 192.168.1.24:3000".to_string();
                }),
            ),
        ],

        // ── 视频导出：覆盖层 5 态（默认 None 只是占位，不是用户看到的界面）──
        DialogType::VideoExport => vec![
            (
                "[准备中]".to_string(),
                setup(|_: &mut Root| {
                    // `overlay = None`：渲染「准备中…」占位（默认状态）
                }),
            ),
            (
                "[导出中]".to_string(),
                setup(|root: &mut Root| {
                    let state = &mut root.state.video_export_dialog;
                    state.overlay = VideoExportOverlayState::Exporting;
                    state.progress = 0.42;
                    state.current_frame = 1512;
                    state.total_frames = 3600;
                    state.render_fps = 57.6;
                    state.elapsed_secs = 26.2;
                    state.status_message = "正在渲染第 1512/3600 帧".to_string();
                }),
            ),
            (
                "[封装中]".to_string(),
                setup(|root: &mut Root| {
                    root.state.video_export_dialog.overlay = VideoExportOverlayState::Finalizing;
                }),
            ),
            (
                "[已完成]".to_string(),
                setup(|root: &mut Root| {
                    root.state.video_export_dialog.overlay = VideoExportOverlayState::Completed {
                        total_frames: 3600,
                        elapsed_secs: 62.5,
                        avg_fps: 57.6,
                    };
                }),
            ),
            (
                "[失败]".to_string(),
                setup(|root: &mut Root| {
                    root.state.video_export_dialog.overlay = VideoExportOverlayState::Error(
                        "ffmpeg 编码失败（退出码 1）：\n无法写入输出文件 \
                         D:\\作品\\2026\\钢琴协奏曲-第四乐章-最终版\\output.mp4\n\
                         原因：目标目录不存在或没有写入权限；请检查磁盘空间与路径后重试。"
                            .to_string(),
                    );
                }),
            ),
        ],

        // ── 云存储：连接面板 / 浏览面板（空、有列表、保存模式）/ 断连提醒 ──
        DialogType::CloudConnect => vec![
            (String::new(), setup(|_: &mut Root| {})),
            (
                "[连接失败]".to_string(),
                setup(|root: &mut Root| {
                    let cloud = &mut root.cloud;
                    cloud.protocol = lumino_message::CloudProtocolUi::Sftp;
                    cloud.name = "我的 NAS".to_string();
                    cloud.address = "nas.example.lan".to_string();
                    cloud.port = "22".to_string();
                    cloud.username = "lumino".to_string();
                    cloud.connect_error = Some(
                        "认证失败：服务器拒绝了用户名或密码（SSH 2.0，错误码 530）".to_string(),
                    );
                }),
            ),
            (
                "[连接中]".to_string(),
                setup(|root: &mut Root| {
                    let cloud = &mut root.cloud;
                    cloud.address = "nas.example.lan".to_string();
                    cloud.connecting = true;
                }),
            ),
        ],
        DialogType::CloudBrowser => vec![
            (String::new(), setup(|_: &mut Root| {})),
            ("[有列表]".to_string(), setup(cloud_browser_filled)),
            ("[保存模式]".to_string(), setup(cloud_browser_save_mode)),
        ],
        DialogType::CloudNotice => vec![
            (String::new(), setup(|_: &mut Root| {})),
            (
                "[长提醒]".to_string(),
                setup(|root: &mut Root| {
                    root.cloud.alert_message = Some(
                    "与「我的 NAS」的连接在 12:04 断开：网络不可达。已自动重试 3 次，全部失败。\
                     请检查服务器地址、端口与防火墙设置后重新连接。"
                        .to_string(),
                );
                }),
            ),
        ],

        // ── 找回音轨：空列表 / 有列表（含选中行）──
        DialogType::RecoverTrack => vec![
            (String::new(), setup(|_: &mut Root| {})),
            ("[有列表]".to_string(), setup(recover_track_filled)),
        ],

        // ── 其余对话框：内容由默认状态唯一决定 ──
        _ => vec![(String::new(), setup(|_: &mut Root| {}))],
    }
}

/// 调试定位：把某先序下标节点的「祖先链 + 兄弟序号」渲染成可读路径
///
/// 形如 `#24[0/2](688×90) → #126[1/5](464×32) → #130[2/5](392×18)`——
/// `[i/n]` 是「在父节点第 i 个兄弟 / 共 n 个」，用来把违规下标直接映射回
/// 视图源码里那一行 `row![...]`。
fn node_path(
    node: &Node,
    target: usize,
    counter: &mut usize,
    path: &mut Vec<String>,
    position: usize,
    siblings: usize,
) -> bool {
    let index = *counter;
    *counter += 1;

    let bounds = node.bounds();
    path.push(format!(
        "#{index}[{position}/{siblings}]({:.0}×{:.0})",
        bounds.width, bounds.height
    ));

    if index == target {
        return true;
    }

    let total = node.children().len();
    for (position, child) in node.children().iter().enumerate() {
        if node_path(child, target, counter, path, position, total) {
            return true;
        }
    }

    path.pop();
    false
}

/// 违规下标 → 可读路径
pub(super) fn describe(node: &Node, target: usize) -> String {
    let mut path = Vec::new();
    let mut counter = 0;
    let _ = node_path(node, target, &mut counter, &mut path, 0, 1);

    path.join(" → ")
}
