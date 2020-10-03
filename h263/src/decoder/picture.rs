//! Decoded picture type

use crate::types::{Picture, SourceFormat};

/// A decoded picture.
pub struct DecodedPicture {
    /// The header of the picture that was decoded.
    picture_header: Picture,

    /// The source format in force for this picture.
    format: SourceFormat,

    /// The luma data of the decoded picture.
    luma: Vec<u8>,

    /// The u-component chroma data of the decoded picture.
    chroma_b: Vec<u8>,

    /// The v-component chroma data of the decoded picture.
    chroma_r: Vec<u8>,
}

impl DecodedPicture {
    /// Construct a new `DecodedPicture` for a given picture in a particular
    /// source format.
    pub fn new(picture_header: Picture, format: SourceFormat) -> Option<Self> {
        let (w, h) = format.into_width_and_height()?;
        let luma_samples = w as usize * h as usize;
        let mut luma = Vec::new();
        luma.resize(luma_samples, 0);

        let chroma_samples = luma_samples / 4;
        let mut chroma_b = Vec::new();
        chroma_b.resize(chroma_samples, 0);
        let mut chroma_r = Vec::new();
        chroma_r.resize(chroma_samples, 0);

        Some(Self {
            picture_header,
            format,
            luma,
            chroma_b,
            chroma_r,
        })
    }

    /// Get the header this picture was decoded with.
    pub fn as_header(&self) -> &Picture {
        &self.picture_header
    }

    /// Get the source format
    pub fn format(&self) -> SourceFormat {
        self.format
    }
}
