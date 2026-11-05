//! slot's label service client (`crates/slot/src/art/` in slot): an index
//! of labels by ROM CRC32, fetched with ETag caching, and the label images
//! it points at turned into Cart Studio-compatible PNGs.
//!
//! Off unless `label_service = on` in settings: slot's art host has not
//! approved third-party use. Local label files always win.

pub mod index;
pub mod jobs;
pub mod make;
