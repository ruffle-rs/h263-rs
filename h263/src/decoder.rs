//! H.263 video decoder.

mod cpu;
mod types;

pub use types::DecoderOption;

use crate::parser::H263Reader;
use std::io::Read;

/// The core decoder structure.
pub struct H263Decoder<R>
where
    R: Read,
{
    reader: H263Reader<R>,
}
