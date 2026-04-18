use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};

use super::types::{TrackBasedCache, TrackCacheHeader, TrackEventWindow, TrackEvents};

mod new;
mod path;
mod read;
mod build;
mod window;
mod has;
mod single;
mod finalize;
mod invalidate;
mod clear;
mod clone;

pub use new::TrackBasedCache as CacheNew;
pub use path::TrackBasedCache as CachePath;
pub use read::TrackBasedCache as CacheRead;
pub use build::TrackBasedCache as CacheBuild;
pub use window::TrackBasedCache as CacheWindow;
pub use has::TrackBasedCache as CacheHas;
pub use single::TrackBasedCache as CacheSingle;
pub use finalize::TrackBasedCache as CacheFinalize;
pub use invalidate::TrackBasedCache as CacheInvalidate;
pub use clear::TrackBasedCache as CacheClear;
pub use clone::TrackBasedCache as CacheClone;