//! `Editor` 的瞬时编辑状态（T-30 slice 2）。
//!
//! 这些字段共同描述「一次未提交的编辑」：远程协作光标、待提交拖拽、待提交复制。
//! 收敛到一个 bundle 后，Editor 的顶层字段减少，状态边界也更清楚。

use std::collections::HashMap;

use lumino_editor_state::DragState;
use lumino_ui_core::geom::Point;

use crate::RemoteSelectionSet;

/// 协作 / 拖拽的瞬时状态。
#[derive(Debug, Default)]
pub struct EditorTransient {
    /// 协作远程用户：用户 ID -> (位置, 颜色, 用户名)。
    pub remote_cursors: HashMap<String, (Point, String, String)>,
    /// 待提交的拖拽（ghost 偏移）。
    pub(crate) pending_drag: Option<DragState>,
    /// 待提交的复制拖拽。
    pub(crate) pending_copy: Option<DragState>,
    /// 远端用户选择集合（user_id -> 指纹 / 时间戳 / 颜色）。
    pub(crate) remote_selections: HashMap<String, RemoteSelectionSet>,
    /// 本地选择会话时间戳；`None` = 没有进行中的选择会话。
    pub(crate) local_selection_timestamp: Option<u64>,
    /// 与 `local_selection_timestamp` 同步写入 / 清空的本地选择指纹。
    pub(crate) local_selection_fingerprints: Vec<(usize, f32, u16, f32)>,
}

impl EditorTransient {
    /// - `active == true`：记录指纹与时间戳，供高亮渲染与冲突判定使用。
    pub fn apply_remote_selection(&mut self, user_id: &str, selection_json: &str, color: &str) {
        let parsed: serde_json::Value = match serde_json::from_str(selection_json) {
            Ok(v) => v,
            Err(e) => {
                tracing::error!("协作: 解析远端选择失败: {}", e);
                return;
            }
        };
        let active = parsed
            .get("active")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !active {
            self.remote_selections.remove(user_id);
            return;
        }
        let timestamp = parsed
            .get("timestamp")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let mut fingerprints = Vec::new();
        if let Some(arr) = parsed.get("fingerprints").and_then(|v| v.as_array()) {
            for f in arr {
                if let (Some(t), Some(ti), Some(k), Some(l)) = (
                    f.get(0).and_then(|v| v.as_f64()),
                    f.get(1).and_then(|v| v.as_f64()),
                    f.get(2).and_then(|v| v.as_f64()),
                    f.get(3).and_then(|v| v.as_f64()),
                ) {
                    fingerprints.push((t as usize, ti as f32, k as u16, l as f32));
                }
            }
        }
        self.remote_selections.insert(
            user_id.to_string(),
            RemoteSelectionSet {
                timestamp,
                fingerprints,
                color: color.to_string(),
            },
        );
    }

    /// 移除远端用户的选择（用户离开等）
    pub fn remove_remote_selection(&mut self, user_id: &str) {
        self.remote_selections.remove(user_id);
    }

    /// first-writer-wins 冲突判定
    ///
    /// 若 `notes` 中任一音符与某个**更早**（`timestamp < local_timestamp`）的
    /// 远端选择指纹重叠（同轨、同键、tick 区间相交），返回 `true`：
    /// 表示远端已先占该音符，本地编辑应让行（远端优先）。
    ///
    /// 改进：过期远端选择（>5 秒未更新）自动失效，避免网络异常或用户挂起导致永久锁定；
    /// 平局（`==`）本地优先，避免同毫秒抖动误锁。
    pub fn is_locked_by_other(
        &self,
        local_timestamp: u64,
        notes: &[(usize, f32, u16, f32)],
    ) -> bool {
        let now = now_ms();
        for sel in self.remote_selections.values() {
            // 过期检查：远端选择超过 5 秒未更新视为失效（用户已释放或网络断开）
            if now.saturating_sub(sel.timestamp) > 5_000 {
                continue;
            }
            if sel.timestamp >= local_timestamp {
                continue; // 远端不更早，不锁定（平局本地赢）
            }
            for (rtrack, rtick, rkey, rlen) in &sel.fingerprints {
                for (track, tick, key, len) in notes {
                    if *rtrack == *track
                        && *rkey == *key
                        && *tick < *rtick + *rlen
                        && *tick + *len > *rtick
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// 用当前本地选择状态判定是否被远端更早选择锁定
    pub(crate) fn local_selection_is_locked(&self) -> bool {
        match self.local_selection_timestamp {
            Some(ts) => self.is_locked_by_other(ts, &self.local_selection_fingerprints),
            None => false,
        }
    }

    /// 判定目标音符区间是否被远端更早选择锁定（供粘贴/插入等非选区操作调用）
    ///
    /// 调用方传入待插入/粘贴的音符指纹列表，若任一目标区间与远端更早选择重叠则返回 true。
    /// 该检查可防止“粘贴覆盖远端已框选音符”的冲突（随机混合编辑场景）。
    #[allow(dead_code)]
    pub fn is_target_locked_by_other(&self, target_notes: &[(usize, f32, u16, f32)]) -> bool {
        let now = now_ms();
        // 用当前时刻作为本地时间戳近似（粘贴瞬时），若远端更早则锁定
        self.is_locked_by_other(now, target_notes)
    }

    /// 广播本地选择变更（供协作客户端转发到其他用户）
    ///
    /// - `active == true`：写入选中指纹 + 时间戳，并作为本地选择会话起点。
    /// - `active == false`：清空本地选择会话并通知对端选择已结束。
    ///
    /// `fingerprints` 仅在 `active == true` 时使用；由调用方从编辑器选区导出。
    pub fn emit_local_selection_changed(
        &mut self,
        active: bool,
        fingerprints: &[(usize, f32, u16, f32)],
    ) {
        let timestamp = if active {
            now_ms()
        } else {
            self.local_selection_timestamp.unwrap_or(0)
        };
        if active {
            self.local_selection_timestamp = Some(timestamp);
            self.local_selection_fingerprints = fingerprints.to_vec();
            let json_fps: Vec<[f64; 4]> = fingerprints
                .iter()
                .map(|(t, ti, k, l)| [*t as f64, *ti as f64, *k as f64, *l as f64])
                .collect();
            lumino_message::events::emit(lumino_message::events::Event::Window(
                lumino_message::events::window::Event::local_selection_changed(
                    true, timestamp, json_fps,
                ),
            ));
        } else {
            self.local_selection_timestamp = None;
            self.local_selection_fingerprints.clear();
            lumino_message::events::emit(lumino_message::events::Event::Window(
                lumino_message::events::window::Event::local_selection_changed(
                    false,
                    timestamp,
                    Vec::new(),
                ),
            ));
        }
    }
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
