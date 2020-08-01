//! Internal reader adapter for reading H.263 bitstreams.

use crate::error::{Error, Result};
use crate::traits::BitReadable;
use std::cmp::min;
use std::collections::VecDeque;
use std::io::Read;

/// A reader that allows decoding an H.263 compliant bitstream.
pub struct H263Reader<R>
where
    R: Read,
{
    /// The data source to read bits from.
    source: R,

    /// Internal buffer storing already-read bytes from the bitstream data.
    buffer: VecDeque<u8>,

    /// How many bits of the head byte in the buffer have already been read.
    ///
    /// If the value is nonzero, then all data in the buffer must be shifted
    /// left by this many bits. Reading a partial number
    ///
    /// If this value is eight or more, then the buffer should have bytes
    /// removed from it, and eight subtracted from this value, until it is less
    /// than eight.
    bits_read: u8,
}

impl<R> H263Reader<R>
where
    R: Read,
{
    /// Wrap a source file in a reader.
    fn from_source(source: R) -> Self {
        Self {
            source,
            buffer: VecDeque::new(),
            bits_read: 0,
        }
    }

    /// Fill the internal read buffer with a given number of bytes.
    ///
    /// This function will yield all I/O errors wrapped inside of the
    /// `UnhandledIoError` variant type.
    fn buffer_bytes(&mut self, bytes_needed: usize) -> Result<()> {
        let mut byte = [0];
        for _ in 0..bytes_needed {
            //TODO: Get a byte, get a byte, get a byte, byte, byte!
            self.source.read_exact(&mut byte[..])?;
            self.buffer.push_back(byte[0]);
        }

        Ok(())
    }

    /// Given a certain number of needed bits, return how many bytes would need
    /// to be buffered to read it.
    fn needed_bytes_for_bits(&mut self, bits_needed: u32) -> usize {
        let bits_available = (self.buffer.len() * 8).saturating_sub(self.bits_read.into());
        let bits_short = (bits_needed as usize).saturating_sub(bits_available);

        (bits_short / 8) + if bits_short % 8 != 0 { 1 } else { 0 }
    }

    fn ensure_bits(&mut self, bits_needed: u32) -> Result<()> {
        let bytes = self.needed_bytes_for_bits(bits_needed);
        self.buffer_bytes(bytes)
    }

    /// Read an arbitrary number of bits out into a type.
    ///
    /// The bits will be returned such that the read-out bits start from the
    /// least significant bit of the returned type. This means that, say,
    /// reading two bits from the bitstream will result in a value that has
    /// been zero-extended.
    ///
    /// The `bits_needed` must not exceed the maximum width of the type. Any
    /// attempt to do so will result in an error.
    fn read_bits<T: BitReadable>(&mut self, mut bits_needed: u32) -> Result<T> {
        if (T::zero().checked_shl(bits_needed)).is_none() {
            return Err(Error::InternalDecoderError);
        }

        self.ensure_bits(bits_needed)?;

        let mut accum = T::zero();
        while bits_needed > 0 {
            let byte = self
                .buffer
                .front()
                .expect("buffer bytes should have been ensured")
                << self.bits_read;
            let bits_in_byte = (8 as u32).saturating_sub(self.bits_read as u32);

            let bits_to_shift_in = min(bits_in_byte, bits_needed);

            accum = (accum << bits_to_shift_in) | (byte >> (8 - bits_to_shift_in)).into();

            self.bits_read += bits_to_shift_in as u8;
            if self.bits_read >= 8 {
                self.bits_read -= 8;
                self.buffer
                    .pop_front()
                    .expect("one buffer byte should have been popped");
            }

            bits_needed = bits_needed.saturating_sub(bits_to_shift_in);
        }

        Ok(accum)
    }
}

#[cfg(test)]
mod tests {
    use crate::read::H263Reader;

    #[test]
    fn read_unaligned_bits() {
        let data = [0xFF, 0x72, 0x1C, 0x1F];
        let mut reader = H263Reader::from_source(&data[..]);

        assert_eq!(0x07, reader.read_bits(3).unwrap());
        assert_eq!(0x3E, reader.read_bits(6).unwrap());
        assert_eq!(0x721C1F, reader.read_bits(23).unwrap());
        reader.read_bits::<u8>(1).unwrap_err();
    }
}
