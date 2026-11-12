//! slot's label service client (`crates/slot/src/art/` in slot): an index
//! of labels by ROM CRC32, fetched with ETag caching, and the label images
//! it points at turned into Cart Studio-compatible PNGs.
//!
//! On unless MENU -> Settings -> Online labels is off (`online_labels =
//! off`). Local label files always win.

pub mod index;
pub mod jobs;
pub mod make;
