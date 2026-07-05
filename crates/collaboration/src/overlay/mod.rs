//! 协作覆盖层系统
//!
//! 管理协作编辑产生的洋葱皮覆盖层（overlay），支持：
//! - 区域增量检测（1s 轮询，只比对单个区域）
//! - 覆盖层生成与合并（2+ 覆盖层自动合并）
//! - 用户离开后的阈值合并到主贴图

mod delta;
mod manager;
mod types;

pub use delta::RegionDeltaDetector;
pub use manager::OverlayManager;
pub use types::{
    DeltaResult, OverlayConfig, OverlayState, OverlayTile, RegionCoord, RegionEditState,
    RegionSnapshot,
};
