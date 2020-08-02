//! Internal reader adapter for reading H.263 bitstreams.

use crate::error::{Error, Result};
use crate::traits::BitReadable;
use std::cmp::min;
use std::collections::VecDeque;
use std::io::Read;

/// A reader that allows decoding an H.263 compliant bitstream.
///
/// This reader implements an internal buffer that can be read from as a series
/// of bits into a number of possible types.
pub struct H263Reader<R>
where
    R: Read,
{
    /// The data source to read bits from.
    source: R,

    /// Internal buffer of already-read bitstream data.
    buffer: VecDeque<u8>,

    /// How many bits of the buffer have already been read.
    ///
    /// If this value modulo eight is nonzero, then reads out of the internal
    /// buffer must read
    bits_read: usize,
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
        let bits_available = (self.buffer.len() * 8).saturating_sub(self.bits_read);
        let bits_short = (bits_needed as usize).saturating_sub(bits_available);

        (bits_short / 8) + if bits_short % 8 != 0 { 1 } else { 0 }
    }

    /// Ensure that at least a certain number of additional bits can be read
    /// from the internal buffer.
    fn ensure_bits(&mut self, bits_needed: u32) -> Result<()> {
        let bytes = self.needed_bytes_for_bits(bits_needed);
        self.buffer_bytes(bytes)
    }

    /// Copy an arbitrary number of bits from the stream out into a type.
    ///
    /// The bits will be returned such that the read-out bits start from the
    /// least significant bit of the returned type. This means that, say,
    /// reading two bits from the bitstream will result in a value that has
    /// been zero-extended.
    ///
    /// This function does not remove bits from the buffer. Repeated calls to
    /// peek_bits return the same bits.
    ///
    /// The `bits_needed` must not exceed the maximum width of the type. Any
    /// attempt to do so will result in an error.
    pub fn peek_bits<T: BitReadable>(&mut self, mut bits_needed: u32) -> Result<T> {
        if (T::zero().checked_shl(bits_needed)).is_none() {
            return Err(Error::InternalDecoderError);
        }

        self.ensure_bits(bits_needed)?;

        let mut accum = T::zero();
        let bytes_read = self.bits_read / 8;
        let mut bits_read = self.bits_read % 8;
        for byte in self.buffer.iter().skip(bytes_read) {
            let byte = byte << bits_read;
            let bits_in_byte = (8 as u32).saturating_sub(bits_read as u32);

            let bits_to_shift_in = min(bits_in_byte, bits_needed);

            accum = (accum << bits_to_shift_in) | (byte >> (8 - bits_to_shift_in)).into();

            bits_read = 0;
            bits_needed = bits_needed.saturating_sub(bits_to_shift_in);
        }

        assert_eq!(
            0, bits_needed,
            "return type accumulator should have been filled"
        );

        Ok(accum)
    }

    /// Skip forward a certain number of bits in the stream buffer.
    ///
    /// If more bits are requested to be skipped than exist within the buffer,
    /// then they will be read in. If this process generates an IO error of any
    /// kind, it will be returned, and no skipping will take place.
    fn skip_bits(&mut self, bits_to_skip: u32) -> Result<()> {
        self.ensure_bits(bits_to_skip)?;

        self.bits_read += bits_to_skip as usize;

        Ok(())
    }

    /// Skip bits until the bitstream is aligned.
    pub fn skip_to_alignment(&mut self) -> Result<()> {
        let misalignment = self.bits_read % 8;

        if misalignment > 0 {
            self.skip_bits(8 - misalignment as u32)
        } else {
            Ok(())
        }
    }

    /// Move an arbitrary number of bits from the stream out into a type.
    ///
    /// This function operates similar to `peek_bits`, but the internal buffer
    /// of this reader will be advanced by the same number of bits that have
    /// been returned.
    pub fn read_bits<T: BitReadable>(&mut self, bits_needed: u32) -> Result<T> {
        let r = self.peek_bits(bits_needed)?;
        self.skip_bits(bits_needed)?;

        Ok(r)
    }

    /// Read a `u8` from the bitstream.
    pub fn read_u8(&mut self) -> Result<u8> {
        self.read_bits(8)
    }

    /// Yield a checkpoint value that can be used to abort a complex read
    /// operation.
    ///
    /// In the event that a read operation fails, the prior state of the
    /// internal buffer may be restored using the returned checkpoint.
    ///
    /// This is not an arbitrary seek mechanism: checkpoints are only valid
    /// for as long as the internal buffer retains the same amount of data, or
    /// more.
    fn checkpoint(&self) -> usize {
        self.bits_read
    }

    /// Restore a previously-created checkpoint.
    ///
    /// Upon restoring a checkpoint, all bits read from this reader will be
    ///
    /// Checkpoints handed to this function must be valid. Specifically, the
    /// internal buffer must not have been cleared (e.g. via `commit`) between
    /// the creation and use of this checkpoint.
    fn rollback(&mut self, checkpoint: usize) -> Result<()> {
        if checkpoint >= (self.buffer.len() * 8) {
            return Err(Error::InternalDecoderError);
        }

        self.bits_read = checkpoint;

        Ok(())
    }

    /// Invalidate any previous checkpoints and discard the internal buffer.
    ///
    /// This should only be called once all of the data necessary to represent
    /// a user-facing object has been read. All existing checkpoints will be
    /// invalidated.
    fn commit(&mut self) {
        self.buffer.drain(0..self.bits_read / 8);
        self.bits_read %= 8;
    }

    /// Run some struct-parsing code in such a way that it will not advance the
    /// bitstream position unless it successfully parses a value.
    ///
    /// Closures passed to this function must yield a `Result`. The buffer
    /// position will not be modified if the function yields an `Err`.
    ///
    /// TODO: This function does not discard successfully parsed buffer data
    /// via `commit` due to the lack of safety tracking on checkpoints. This
    /// function should be reentrant.
    pub fn with_transaction<F, T>(&mut self, f: F) -> Result<T>
    where
        F: FnOnce(&mut Self) -> Result<T>,
    {
        let checkpoint = self.checkpoint();

        let result = f(self);

        if result.is_err() {
            self.rollback(checkpoint)?;
        }

        result
    }

    /// Run some struct-parsing code in such a way that it will not advance the
    /// bitstream position unless it successfully parses a value.
    ///
    /// Closures passed to this function must yield an `Option`, wrapped in a
    /// `Result`. The buffer position will not be modified if the function
    /// yields `Err` or `None`. Use `None` to signal that the desired data does
    /// not exist in the bitstream (e.g. for data that could be one of multiple
    /// types)
    ///
    /// TODO: This function does not discard successfully parsed buffer data
    /// via `commit` due to the lack of safety tracking on checkpoints. This
    /// function should be reentrant.
    pub fn with_transaction_option<F, T>(&mut self, f: F) -> Result<Option<T>>
    where
        F: FnOnce(&mut Self) -> Result<Option<T>>,
    {
        let checkpoint = self.checkpoint();

        let result = f(self);

        match &result {
            Ok(None) | Err(_) => self.rollback(checkpoint)?,
            _ => {}
        };

        result
    }
}

#[cfg(test)]
mod tests {
    use crate::decoder::reader::H263Reader;

    #[test]
    fn read_unaligned_bits() {
        let data = [0xFF, 0x72, 0x1C, 0x1F];
        let mut reader = H263Reader::from_source(&data[..]);

        assert_eq!(0x07, reader.read_bits(3).unwrap());
        assert_eq!(0x3E, reader.read_bits(6).unwrap());
        assert_eq!(0x721C1F, reader.read_bits(23).unwrap());
        reader.read_bits::<u8>(1).unwrap_err();
    }

    #[test]
    fn peek_bits() {
        let data = [0xFF, 0x72, 0x1C, 0x1F];
        let mut reader = H263Reader::from_source(&data[..]);

        assert_eq!(0x07, reader.peek_bits(3).unwrap());
        assert_eq!(0x3F, reader.peek_bits(6).unwrap());
        assert_eq!(0x7FB90E, reader.peek_bits(23).unwrap());
        reader.peek_bits::<u64>(64).unwrap_err();
    }
}
