//! Decoded macroblock type

/// A (partially) decoded macroblock, consisting of it's four luma blocks and
/// single chroma block.
pub struct DecodedMacroblock {
    luma: [[u8; 64]; 4],

    chroma_b: [u8; 64],

    chroma_r: [u8; 64],
}

impl DecodedMacroblock {
    pub fn new() -> Self {
        Self {
            luma: [[0; 64]; 4],
            chroma_b: [0; 64],
            chroma_r: [0; 64],
        }
    }

    pub fn as_luma(&self, index: usize) -> &[u8; 64] {
        &self.luma[index]
    }

    pub fn luma_mut(&mut self, index: usize) -> &mut [u8; 64] {
        &mut self.luma[index]
    }

    pub fn as_chroma_b(&self) -> &[u8; 64] {
        &self.chroma_b
    }

    pub fn chroma_b_mut(&mut self) -> &mut [u8; 64] {
        &mut self.chroma_b
    }

    pub fn as_chroma_r(&self) -> &[u8; 64] {
        &self.chroma_r
    }

    pub fn chroma_r_mut(&mut self) -> &mut [u8; 64] {
        &mut self.chroma_r
    }
}

impl Default for DecodedMacroblock {
    fn default() -> Self {
        Self::new()
    }
}
