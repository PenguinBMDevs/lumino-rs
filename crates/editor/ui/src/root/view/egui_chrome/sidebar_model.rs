//! 左侧栏 → egui 视图模型。
//!
//! 路由条目的可见性 / 点亮规则只有这一处实现，顺序即 `ROUTES` 顺序，绘制与命中
//! 不会错位。本片只构造模型，不画像素。

use lumino_core::i18n::{Language, main_translations};
use lumino_core::tr;
use lumino_ui_egui::chrome::{
    RollBar, SidebarGroup, SidebarModel, SidebarPanel, SidebarRouteIcon, SidebarRouteItem,
    SidebarTrack,
};
use lumino_ui_egui::widgets::HoverTip;

use crate::sidebar::{RollBarButton, Route, Sidebar};

use super::{color32, sidebar_group, sidebar_panel};

impl Sidebar {
    /// 构造左侧栏只读模型；[`Self::has_transient_ui`] 为真时返回 `None`
    ///（叠加层不绘制左栏，避免盖住其他通道的绘制）。
    pub(super) fn egui_model(
        &self,
        language: Language,
        mixer_open: bool,
        waterfall: bool,
    ) -> Option<SidebarModel> {
        if self.has_transient_ui() {
            return None;
        }
        let translations = main_translations(language);

        let panel = match self.panel_route {
            Route::File => SidebarPanel::Tracks,
            Route::Arrangement => SidebarPanel::AddTrackOnly,
            _ => SidebarPanel::Blank,
        };

        // 生产 `route.rs` 的 `Route::File` 子按钮规则：
        // `active == Arrangement` 时不点亮，否则 `panel_visible && route == File`。
        let file_active =
            self.route != Route::Arrangement && self.panel_visible && self.route == Route::File;

        let tracks = self.track_models();
        let active_group = self.active_group.map(sidebar_group);
        let route_items = self.route_items(language, file_active, active_group, waterfall);

        Some(SidebarModel {
            panel_visible: self.panel_visible,
            panel_width: self.panel_width,
            piano_roll_visible: self.piano_roll_visible,
            waterfall,
            active_group,
            route_items,
            automation_panel_visible: self.automation_panel_visible,
            audio_export_visible: self.audio_export_visible,
            video_export_visible: self.video_export_visible,
            file_active,
            roll_bar_visible: false,
            roll_bar_active: match self.roll_bar_active {
                Some(RollBarButton::Horizontal) => Some(RollBar::Horizontal),
                Some(RollBarButton::Vertical) => Some(RollBar::Vertical),
                None => None,
            },
            roll_bar_horizontal_tip: HoverTip::new(
                tr!("横向卷帘"),
                tr!("钢琴卷帘横向显示（时间轴水平）"),
            ),
            roll_bar_vertical_tip: HoverTip::new(
                tr!("纵向卷帘"),
                tr!("钢琴卷帘纵向显示（时间轴垂直）"),
            ),
            mixer_tip: HoverTip::new(tr!("混音台"), tr!("打开 / 关闭混音台窗口")),
            solo_tip: HoverTip::new(tr!("独奏"), tr!("只播放此音轨；再次点击取消")),
            mixer_open,
            panel,
            tracks,
            track_list_title: translations.sidebar_track_list.to_owned(),
            add_track_label: translations.sidebar_add_track.to_owned(),
            context_menu: sidebar_panel::track_menu(self),
            panel_menu: sidebar_panel::panel_menu(self),
            color_picker: sidebar_panel::color_picker(self),
            numeric_picker: sidebar_panel::numeric_picker(self),
            resizing: self.is_resizing,
            reorder: self
                .track_reorder
                .as_ref()
                .filter(|state| state.active)
                .map(|state| lumino_ui_egui::chrome::SidebarReorderModel {
                    track_id: state.track_id,
                    hover_index: state.hover_index,
                }),
        })
    }

    /// 路由栏条目：可见性 / 点亮规则与 上一代 `route.rs` 同口径。
    fn route_items(
        &self,
        language: Language,
        file_active: bool,
        active_group: Option<SidebarGroup>,
        waterfall: bool,
    ) -> Vec<SidebarRouteItem> {
        let translations = main_translations(language);
        let mut route_items = vec![SidebarRouteItem {
            icon: SidebarRouteIcon::PianoRoll,
            active: self.piano_roll_visible,
            tip: HoverTip::new(tr!("钢琴卷帘"), tr!("打开钢琴卷帘编辑视图")),
        }];
        if active_group == Some(SidebarGroup::PianoRoll) {
            route_items.push(SidebarRouteItem {
                icon: SidebarRouteIcon::File,
                active: file_active,
                tip: HoverTip::new(translations.sidebar_file, tr!("显示工程的全部音轨")),
            });
            route_items.push(SidebarRouteItem {
                icon: SidebarRouteIcon::Automation,
                active: self.automation_panel_visible,
                tip: HoverTip::new(
                    translations.sidebar_automation,
                    tr!("显示当前音轨的自动化曲线"),
                ),
            });
        }
        route_items.push(SidebarRouteItem {
            icon: SidebarRouteIcon::Project,
            active: active_group == Some(SidebarGroup::Project),
            tip: HoverTip::new(tr!("音轨总览"), tr!("切换到工程走带 / 音轨总览视图")),
        });
        route_items.push(SidebarRouteItem {
            icon: SidebarRouteIcon::Waterfall,
            active: waterfall,
            tip: HoverTip::new(tr!("瀑布流播放器"), tr!("切换到瀑布流播放视图")),
        });
        route_items.push(SidebarRouteItem {
            icon: SidebarRouteIcon::Renderer,
            active: active_group == Some(SidebarGroup::Renderer),
            tip: HoverTip::new(tr!("渲染"), tr!("打开渲染入口（视频 / 音频导出）")),
        });
        if active_group == Some(SidebarGroup::Renderer) {
            route_items.push(SidebarRouteItem {
                icon: SidebarRouteIcon::VideoExport,
                active: self.video_export_visible,
                tip: HoverTip::new(tr!("视频导出"), tr!("把工程渲染为视频文件")),
            });
            route_items.push(SidebarRouteItem {
                icon: SidebarRouteIcon::AudioExport,
                active: self.audio_export_visible,
                tip: HoverTip::new(tr!("音频导出"), tr!("把工程渲染为音频文件")),
            });
        }
        route_items
    }

    /// 音轨列表：重命名态由 `renaming_track` 推导，不另存一份。
    fn track_models(&self) -> Vec<SidebarTrack> {
        self.tracks
            .iter()
            .map(|track| SidebarTrack {
                id: track.id,
                label: track.display_label.clone(),
                name: track.name.clone(),
                color: track.color.map(color32),
                selected: track.id == self.selected_track,
                conductor: track.is_conductor,
                soloed: track.is_soloed,
                renaming: self
                    .renaming_track
                    .as_ref()
                    .is_some_and(|(id, _)| *id == track.id),
                rename_buffer: self
                    .renaming_track
                    .as_ref()
                    .filter(|(id, _)| *id == track.id)
                    .map(|(_, buf)| buf.clone())
                    .unwrap_or_default(),
            })
            .collect()
    }
}
