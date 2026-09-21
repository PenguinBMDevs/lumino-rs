//! True streaming MIDI — file is never fully loaded.
//!
//! Old `MidiStream` held `_raw: Vec<u8>` (800 MB) + `Smf` tracks (≈1 GB) +
//! `Vec<TimedEvent>` (1.6 GB) → >3 GB. This rewrite is `O(tracks + block)`:
//! header + track offsets are read via `memmap2` (zero-copy, dropped after
//! scan), each track is streamed via 8 KiB `BufReader<File>`; heap holds one
//! event per track.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use memmap2::Mmap;

use crate::SynthError;
use crate::midi::{TimedEvent, kind};

mod midi;
mod parse;
mod tempo;
mod track;

pub use midi::MidiStream;

#[cfg(test)]
mod tests;
