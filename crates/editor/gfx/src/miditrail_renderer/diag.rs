use super::*;

impl MiditrailRenderer {
    /// 换算段打点（首 3 帧 + 每 300 帧）：`render_inner` 内部打点见 `diag_stages`。
    pub(super) fn diag_convert(convert_us: u64, notes: usize) {
        static COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if n < 3 || n.is_multiple_of(300) {
            tracing::info!("miditrail换算打点[{n}]: convert={convert_us}us notes={notes}");
        }
    }

    /// GPU-Driven 分段打点（首 3 帧 + 每 300 帧）：与 legacy `diag_stages` 对齐口径。
    pub(super) fn diag_driven(
        scan_us: u64,
        upload_us: u64,
        aura_us: u64,
        submit_us: u64,
        notes: usize,
    ) {
        static COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if n < 3 || n.is_multiple_of(300) {
            tracing::info!(
                "miditrail驱动[{n}]: scan={scan_us} upload={upload_us} aura={aura_us} submit={submit_us} notes={notes}"
            );
        }
    }

    /// 渲染内阶段打点（首 3 帧 + 每 300 帧）：拆 render 42ms 黑盒，下一刀的靶子。
    pub(super) fn diag_stages(
        active_us: u64,
        build_notes_us: u64,
        build_keys_us: u64,
        upload_notes_us: u64,
        aura_us: u64,
        submit_us: u64,
        notes: usize,
    ) {
        static COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if n < 3 || n.is_multiple_of(300) {
            tracing::info!(
                "miditrail阶段A[{n}]: active={active_us} build_notes={build_notes_us} build_keys={build_keys_us}"
            );
            tracing::info!(
                "miditrail阶段B[{n}]: upload={upload_notes_us} aura={aura_us} submit={submit_us} notes={notes}"
            );
        }
    }
}
