//! 触摸（Touch）手势层 —— 纯触屏平台（Android）的指针合成与多指手势
//!
//! 背景（源码级证据）：
//! - winit 0.30.13 的 Android 后端**只**投递 `WindowEvent::Touch`
//!   （`winit-0.30.13/src/platform_impl/android/mod.rs:376-423`，`_ => None // TODO mouse events`），
//!   既没有 `CursorMoved` / `MouseInput` / `MouseWheel`，也没有
//!   `CursorEntered|CursorLeft` / `PinchGesture`；
//! - 所以「按下 / 拖动 / 抬起」「滚动」「捏合缩放」「右键」四条链路在 Android 上
//!   全靠本模块合成，任何一条漏了就是真机上对应的交互整块失效。
//!
//! 本模块负责：
//! 1. **单指 → 左键指针**：合成按下/移动/抬起，并把按下/抬起**同步**给
//!    `Host::handle_mouse_input_event`（全局释放兜底、`is_mouse_pressed`、
//!    分割条/侧栏拖拽收尾都挂在那条路径上，见 `host/event/window/mouse.rs:9-70`）；
//!    `MouseInput` 在触屏上永远不来，不同步则这些收尾逻辑整块是死代码。
//! 2. **双指 → 缩放 + 平移**：两指距离 → 缩放（与触控板 `PinchGesture` 共用
//!    `Host::handle_pinch_gesture`）；两指中点位移 → 滚轮像素增量
//!    （`WheelScrolled{Pixels}`，画布与 iced `scrollable` 都吃这条增量）。
//! 3. **手势期消费**：双指期与「双指降单指」期一律消费触摸，不产生指针事件。
//!    否则第二指先抬起时会被合成成一次左键释放（中途误收尾），随后剩余手指
//!    的移动又变成拖拽（误拖音符）。
//!
//! 子模块：
//! - `self::long_press` — 长按模拟右键（触屏没有右键按键）
//!
//! ⚠️ 这里刻意**不**把原生 `Event::Touch` 一并投给 iced：iced 的 button /
//! slider / mouse_area / text_input 等控件同时处理 Mouse 与 Touch 两条分支
//! （`widget/src/button.rs:301-333`），同时投递会让一次点按触发两次。
//! 代价是 `iced_widget::scrollable` 的原生触摸拖动被屏蔽，故改用「双指平移
//! → WheelScrolled」覆盖滚动需求。

mod long_press;

#[cfg(test)]
mod tests;

use iced_core::{Point, mouse};
use iced_winit::{conversion, winit};

use crate::host::Host;

/// 双指平移转滚动的死区（逻辑像素）：低于此值视为手指抖动，不产生滚动
const PAN_DEADZONE: f32 = 1.5;

/// 长按允许的手指抖动半径（逻辑像素）：超出即判定为拖动，取消长按
const LONG_PRESS_SLOP: f32 = 8.0;

/// 双指缩放阈值：两指距离相对**上次生效点**变化超过该比例才发一次缩放
///
/// 必须用「阈值 + 重置锚点」而不是「逐帧比较」：逐帧比较时慢速捏合每帧
/// 距离变化都小于阈值，`prev` 又被每帧更新，结果**慢速捏合永远不缩放**。
const PINCH_THRESHOLD_RATIO: f32 = 0.02;

/// 单次缩放增量上限（与 `handle_pinch_gesture` 的 0.85~1.15 夹取对齐）
const PINCH_MAX_DELTA: f32 = 0.15;

/// 一个触摸事件的处理结论
pub(super) enum TouchRouting {
    /// 单指指针：交给 `convert_touch_to_mouse` 合成左键事件
    Pointer,
    /// 已由手势层消费：不得产生任何指针事件
    Consumed,
    /// 长按已模拟过右键：抬手只补右键释放，不再合成左键事件
    RightRelease,
}

/// 双指缩放增量：相对锚点的距离比，未越过阈值返回 `None`
///
/// 返回值为「距离比 − 1」，正数=张开（放大），负数=收拢（缩小）。
fn pinch_ratio(anchor: f32, distance: f32) -> Option<f32> {
    if anchor <= 0.0 || distance <= 0.0 {
        return None;
    }
    let ratio = distance / anchor - 1.0;
    if ratio.abs() < PINCH_THRESHOLD_RATIO {
        None
    } else {
        Some(ratio.clamp(-PINCH_MAX_DELTA, PINCH_MAX_DELTA))
    }
}

/// 两点欧氏距离的平方（比较阈值用，省一次开方）
fn squared_distance(a: Point, b: Point) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy
}

/// 双指平移增量：中点位移是否越过死区，越过则返回像素增量
///
/// 约定与桌面滚轮一致：手指下移（y 增大）→ `delta.y` 为正 → 内容跟随手指下移。
/// `grid/program.rs:123-153` 的 `target = base - delta` 与
/// `widget/src/scrollable.rs:875-877` 的 `-delta` 都满足该方向。
fn pan_increment(previous: Point, current: Point) -> Option<(f32, f32)> {
    let dx = current.x - previous.x;
    let dy = current.y - previous.y;
    if dx.abs() < PAN_DEADZONE && dy.abs() < PAN_DEADZONE {
        None
    } else {
        Some((dx, dy))
    }
}

impl Host {
    /// 处理一个 winit 触摸事件（Android/触屏共用的唯一入口）
    ///
    /// 返回 `TouchRouting::Consumed` / `TouchRouting::RightRelease` 时，调用方
    /// **不得**再走 iced 的通用事件转换管线（前者已由手势层吸收，后者需要
    /// 单独补右键释放）。
    pub(super) fn handle_touch(&mut self, touch: &winit::event::Touch) -> TouchRouting {
        let scale = self.window_ctx.window.scale_factor() as f32;
        let logical = conversion::cursor_position(touch.location, scale);

        // 按下瞬间把光标**钉到触点**：不依赖上游 `cursor_moved` 的早退条件，
        // 保证本批事件里的按下一定能被 press-only 控件（pick_list / menu
        // overlay）按 `cursor.is_over(bounds)` 命中。
        if matches!(touch.phase, winit::event::TouchPhase::Started) {
            self.window_ctx.cursor_position = Some(logical);
            self.window_ctx.cursor = mouse::Cursor::Available(logical);
        }

        match touch.phase {
            winit::event::TouchPhase::Started => {
                self.active_touches.insert(touch.id, logical);

                if self.active_touches.len() >= 2 {
                    // 第二指落下：本次触摸会话升级为手势。
                    // 第一指已经合成过一次左键按下（无法预知后续会有第二指），
                    // 该按下会在最后一指抬起时统一释放（见 Ended 分支）。
                    self.touch_gesture_active = true;
                    self.pinch_anchor_distance = None;
                    self.prev_pan_center = None;
                    // 双指不是长按（否则两指停 500ms 会弹右键菜单）
                    self.cancel_long_press();
                    // 立刻把光标锚到两指中点（`window_manager` 已按本指位置推过一次
                    // 光标，直接用会让锚点贴到某一根手指上）
                    self.apply_multi_touch_gesture();
                    log::debug!(
                        "[TOUCH] started 升手势 fingers={}",
                        self.active_touches.len()
                    );
                    return TouchRouting::Consumed;
                }

                // 首指：同步全局鼠标按下（分割条/侧栏拖拽收尾、is_mouse_pressed）
                self.sync_global_pointer(true);
                self.begin_long_press(logical);
                log::debug!(
                    "[TOUCH] started 单指 pos=({:.0},{:.0}) -> Pointer",
                    logical.x,
                    logical.y
                );
                TouchRouting::Pointer
            }
            winit::event::TouchPhase::Moved => {
                match self.active_touches.get_mut(&touch.id) {
                    Some(p) => *p = logical,
                    None => {
                        // 兜底：漏了 Started（例如应用从后台恢复）也要能接上
                        self.active_touches.insert(touch.id, logical);
                    }
                }

                if self.active_touches.len() >= 2 {
                    self.touch_gesture_active = true;
                    self.cancel_long_press();
                    self.apply_multi_touch_gesture();
                    return TouchRouting::Consumed;
                }

                if self.touch_gesture_active {
                    // 双指降为单指：继续按手势消费，否则剩余手指的移动会变成拖拽
                    self.pinch_anchor_distance = None;
                    self.prev_pan_center = None;
                    return TouchRouting::Consumed;
                }

                // 单指移动超过抖动半径 = 拖动，取消长按
                if let Some(origin) = self.touch_press_origin
                    && squared_distance(origin, logical) > LONG_PRESS_SLOP * LONG_PRESS_SLOP
                {
                    self.cancel_long_press();
                }

                TouchRouting::Pointer
            }
            winit::event::TouchPhase::Ended | winit::event::TouchPhase::Cancelled => {
                self.active_touches.remove(&touch.id);

                // 手势态下的中间状态一律清零，避免用旧锚点算出跳变增量
                self.pinch_anchor_distance = None;
                self.prev_pan_center = None;

                if !self.active_touches.is_empty() {
                    // 还有手指按着：**不能**发左键释放，否则编辑/拖拽被中途收尾
                    self.cancel_long_press();
                    return TouchRouting::Consumed;
                }

                // 全部抬起：手势结束、释放左键、抑制 hover
                self.touch_gesture_active = false;
                let long_pressed = self.touch_long_press_fired;
                self.touch_long_press_deadline = None;
                self.touch_long_press_fired = false;
                self.touch_press_origin = None;
                self.sync_global_pointer(false);
                // 纯触屏没有 CursorLeft，抬手后 iced 的 cursor 仍停在落点，
                // 悬停高亮 / tooltip / 音符 hover 预览会一直挂着 —— 置位由
                // `process_pending_events` 在下一帧以 Unavailable 重建一次。
                self.suppress_hover_after_touch = true;

                if long_pressed {
                    // 长按触发时左键已经结束，这里只需要释放模拟的右键
                    TouchRouting::RightRelease
                } else {
                    TouchRouting::Pointer
                }
            }
        }
    }

    /// 把合成指针的按下/抬起同步给窗口级鼠标处理
    ///
    /// 触屏上 `WindowEvent::MouseInput` 永不出现，若不在此同步：
    /// - `is_mouse_pressed` 恒为 false → 素材拖出跟随每次消息都被判为"已松手"而取消；
    /// - 分割条/左栏/右栏拖拽的全局收尾不执行 → 拖到别处松手后卡在拖拽态；
    /// - 画布内按下、画布外松手的兜底不执行 → 批量拖动/复制卡死。
    fn sync_global_pointer(&mut self, pressed: bool) {
        let state = if pressed {
            winit::event::ElementState::Pressed
        } else {
            winit::event::ElementState::Released
        };
        self.handle_mouse_input_event(state, winit::event::MouseButton::Left);
    }

    /// 双指手势：缩放（距离比）+ 平移（中点位移）
    ///
    /// 两指以下或三指以上都不做手势计算：
    /// - 三指及以上时 `active_touches` 是 HashMap，取「前两个」每帧的配对会跳变，
    ///   算出来的距离比是噪声 → 直接清零锚点、只保留消费。
    fn apply_multi_touch_gesture(&mut self) {
        if self.active_touches.len() != 2 {
            self.pinch_anchor_distance = None;
            self.prev_pan_center = None;
            return;
        }

        // 用切片模式解构而非下标访问：长度已判等 2，但下标写法在后续改动
        // 里很容易变成 panic 点，模式匹配由编译器兜底。
        let points: Vec<Point> = self.active_touches.values().copied().collect();
        let [a, b] = points.as_slice() else {
            return;
        };
        let (a, b) = (*a, *b);
        let center = Point::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
        let distance = {
            let dx = a.x - b.x;
            let dy = a.y - b.y;
            (dx * dx + dy * dy).sqrt().max(1.0)
        };

        // 光标锚到双指中点：缩放的锚点区域判定与滚动的目标控件判定都靠它
        self.window_ctx.cursor_position = Some(center);
        self.window_ctx.cursor = mouse::Cursor::Available(center);

        // ① 缩放：距离相对锚点的变化超过阈值才发一次，并发完即重置锚点
        match self.pinch_anchor_distance {
            Some(anchor) => {
                if let Some(ratio) = pinch_ratio(anchor, distance) {
                    self.handle_pinch_gesture(f64::from(ratio), winit::event::TouchPhase::Moved);
                    self.pinch_anchor_distance = Some(distance);
                }
            }
            None => self.pinch_anchor_distance = Some(distance),
        }

        // ② 平移：中点位移越过死区才产生一次滚轮像素增量
        //    （`prev_pan_center` 只在真正发出增量时前移，慢速拖动可累积）
        let pan = self
            .prev_pan_center
            .and_then(|previous| pan_increment(previous, center));
        if let Some((dx, dy)) = pan {
            self.events
                .push(iced_core::Event::Mouse(mouse::Event::WheelScrolled {
                    delta: mouse::ScrollDelta::Pixels { x: dx, y: dy },
                }));
            self.prev_pan_center = Some(center);
            self.ui_dirty = true;
            self.window_ctx.window.request_redraw();
        } else if self.prev_pan_center.is_none() {
            self.prev_pan_center = Some(center);
        }
    }
}
