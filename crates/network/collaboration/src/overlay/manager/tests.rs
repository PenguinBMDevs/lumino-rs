//! 覆盖层管理器测试

use std::collections::HashMap;

use crate::overlay::manager::OverlayManager;
use crate::overlay::types::{OverlayConfig, OverlayState, RegionCoord};

fn default_config() -> OverlayConfig {
    OverlayConfig::default()
}

#[test]
fn test_overlay_manager_new() {
    let mgr = OverlayManager::new(default_config());
    assert_eq!(mgr.region_count(), 0);
}

#[test]
fn test_generate_overlay_creates_tile() {
    let mut mgr = OverlayManager::new(default_config());
    let coord = RegionCoord::new(0, 0);

    mgr.generate_overlay(&coord);
    assert_eq!(mgr.region_count(), 1);

    let overlays = mgr.get_overlays(&coord);
    assert!(overlays.is_some());
    assert_eq!(overlays.expect("生成一个 overlay 后应有值").len(), 1);
}

#[test]
fn test_generate_overlay_version() {
    let mut mgr = OverlayManager::new(default_config());
    let coord = RegionCoord::new(0, 0);

    mgr.generate_overlay(&coord);
    let v1 = mgr.get_overlays(&coord).expect("应有 overlay")[0].version;

    mgr.generate_overlay(&coord);
    let v2 = mgr.get_overlays(&coord).expect("两次生成应有两个 overlay")[1].version;

    assert!(v2 > v1);
}

#[test]
fn test_generate_overlay_increments_version() {
    let mut mgr = OverlayManager::new(default_config());
    let coord = RegionCoord::new(0, 0);

    mgr.generate_overlay(&coord);
    mgr.generate_overlay(&coord);

    let overlays = mgr.get_overlays(&coord).expect("两次生成后应有 overlay");
    assert_eq!(overlays.len(), 2);
    assert_eq!(overlays[0].version, 1);
    assert_eq!(overlays[1].version, 2);
}

#[test]
fn test_merge_overlays_combines_two() {
    let mut mgr = OverlayManager::new(default_config());
    let coord = RegionCoord::new(0, 0);

    mgr.generate_overlay(&coord);
    mgr.generate_overlay(&coord);

    // Both overlays have empty pixels, but merge should work
    mgr.merge_overlays(&coord);

    let state = mgr.get_state(&coord);
    assert_eq!(state, OverlayState::Merged);

    let overlays = mgr.get_overlays(&coord);
    assert!(overlays.is_some());
    assert_eq!(overlays.expect("merge 后应仍有 overlay").len(), 1);
}

#[test]
fn test_merge_overlays_requires_two() {
    let mut mgr = OverlayManager::new(default_config());
    let coord = RegionCoord::new(0, 0);

    mgr.generate_overlay(&coord);
    mgr.merge_overlays(&coord); // Only 1 overlay, should be no-op

    let state = mgr.get_state(&coord);
    assert_eq!(state, OverlayState::SingleOverlay);
}

#[test]
fn test_update_user_count_triggers_flush_check() {
    let mut mgr = OverlayManager::new(default_config());
    let coord = RegionCoord::new(0, 0);

    // Need 2 overlays for merge to consolidate
    mgr.generate_overlay(&coord);
    mgr.generate_overlay(&coord);
    mgr.merge_overlays(&coord);
    assert_eq!(mgr.get_state(&coord), OverlayState::Merged);

    // Simulate: user present (count=1), then user leaves (count=0)
    // The transition 1→0 triggers PendingFlush
    mgr.update_user_count(&coord, 1);
    mgr.update_user_count(&coord, 0);
    let state = mgr.get_state(&coord);
    assert!(matches!(state, OverlayState::PendingFlush { .. }));
}

#[test]
fn test_check_flush_ready_after_threshold() {
    use std::time::{SystemTime, UNIX_EPOCH};

    let mut mgr = OverlayManager::new(default_config());
    let coord = RegionCoord::new(0, 0);

    mgr.generate_overlay(&coord);
    mgr.generate_overlay(&coord);
    mgr.merge_overlays(&coord);

    // Simulate user transition 1→0 at current time
    mgr.update_user_count(&coord, 1);
    mgr.update_user_count(&coord, 0);

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    // Immediately check - shouldn't be ready
    let ready = mgr.check_flush_ready(now);
    assert!(ready.is_empty());

    // After threshold has elapsed
    let future = now + mgr.config.flush_threshold_ms + 100;
    let ready = mgr.check_flush_ready(future);
    assert_eq!(ready, vec![coord]);
    assert_eq!(mgr.region_count(), 0);
}

#[test]
fn test_poll_no_ops_no_dirty_region() {
    let mut mgr = OverlayManager::new(default_config());
    let coord = RegionCoord::new(0, 0);
    let mut users = HashMap::new();
    users.insert(coord, 1u32);

    // First poll with active user but no operations -> no delta
    let dirty = mgr.poll(&[], 1000, &users);
    assert!(dirty.is_empty());
    // Poll doesn't create regions, it only checks delta on active ones
}
