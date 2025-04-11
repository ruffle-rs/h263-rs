//! Pure-rust H.263 decoder

#[macro_use]
extern crate bitflags;

mod decoder;
mod error;
pub mod parser;
mod traits;
mod types;

pub use decoder::{DecoderOption, H263State};
pub use error::{Error, Result};
pub use types::{PictureOption, PictureTypeCode};
