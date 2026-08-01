use std::{
    collections::VecDeque,
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::util::ReadWriteAtomicU64;

const NPS_WINDOW_MILLISECONDS: u64 = 20;

struct NpsWindow {
    time: u64,
    notes: u64,
}

/// Tracks estimated notes-per-second for realtime note limiting.
pub(super) struct RoughNpsTracker {
    rough_time: Arc<ReadWriteAtomicU64>,
    last_time: u64,
    windows: VecDeque<NpsWindow>,
    total_window_sum: u64,
    current_window_sum: u64,
    stop: Arc<AtomicBool>,
    join_handle: Option<JoinHandle<()>>,
}

impl RoughNpsTracker {
    pub(super) fn disabled() -> RoughNpsTracker {
        RoughNpsTracker {
            rough_time: Arc::new(ReadWriteAtomicU64::new(0)),
            last_time: 0,
            windows: VecDeque::new(),
            total_window_sum: 0,
            current_window_sum: 0,
            stop: Arc::new(AtomicBool::new(true)),
            join_handle: None,
        }
    }

    pub(super) fn new() -> Result<RoughNpsTracker, io::Error> {
        let rough_time = Arc::new(ReadWriteAtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));

        let join_handle = {
            let rough_time = rough_time.clone();
            let stop = stop.clone();
            thread::Builder::new()
                .name("xsynth_nps_tracker".to_string())
                .spawn(move || {
                    let mut last_time = 0;
                    let mut now = Instant::now();
                    while !stop.load(Ordering::Acquire) {
                        thread::sleep(Duration::from_millis(NPS_WINDOW_MILLISECONDS));
                        let diff = now.elapsed();
                        last_time += diff.as_millis() as u64;
                        rough_time.write(last_time);
                        now = Instant::now();
                    }
                })?
        };

        Ok(RoughNpsTracker {
            rough_time,
            last_time: 0,
            windows: VecDeque::new(),
            total_window_sum: 0,
            current_window_sum: 0,
            stop,
            join_handle: Some(join_handle),
        })
    }

    pub(super) fn calculate_nps(&mut self) -> u64 {
        self.check_time();

        loop {
            let cutoff = self.last_time.saturating_sub(1000);
            if let Some(window) = self.windows.front() {
                if window.time < cutoff {
                    self.total_window_sum -= window.notes;
                    self.windows.pop_front();
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        let short_nps = self.current_window_sum * (1000 / NPS_WINDOW_MILLISECONDS) * 4 / 3;
        let long_nps = self.total_window_sum;

        short_nps.max(long_nps)
    }

    pub(super) fn add_note(&mut self) {
        self.current_window_sum += 1;
        self.total_window_sum += 1;
    }

    fn check_time(&mut self) {
        let time = self.rough_time.read();
        if time > self.last_time {
            self.windows.push_back(NpsWindow {
                time: self.last_time,
                notes: self.current_window_sum,
            });
            self.current_window_sum = 0;
            self.last_time = time;
        }
    }
}

impl Drop for RoughNpsTracker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.join_handle.take() {
            if handle.join().is_err() {
                eprintln!("xsynth-realtime: nps tracker thread panicked during shutdown");
            }
        }
    }
}

pub(super) fn should_send_for_vel_and_nps(vel: u8, nps: u64, max: u64) -> bool {
    (vel as u64) * max / 127 > nps
}
