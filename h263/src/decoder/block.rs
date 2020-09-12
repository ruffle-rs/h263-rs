//! Block decoding

use crate::decoder::reader::H263Reader;
use crate::error::{Error, Result};
use crate::types::{Block, IntraDC, MacroblockType, PictureOption, TCoefficient};
use crate::vlc::{Entry, Entry::*};
use std::io::Read;

/// Represents a partially decoded short `TCOEF` entry.
#[derive(Clone, Debug)]
enum ShortTCoefficient {
    /// Indicates that a long `TCOEF` entry follows in the bitstream.
    EscapeToLong,

    /// An almost-fully-decoded short `TCOEF` entry.
    Run {
        last: bool,

        /// The size of the zero-coefficient run.
        run: u8,

        /// The magnitude of the non-zero coefficient.
        ///
        /// It's sign bit directly follows in the bitstream.
        level: u8,
    },
}

use ShortTCoefficient::*;

/// The table of TCOEF values.
///
/// `ESCAPE` is encoded as an `EscapeToLong`, the actual coded values are not
/// decoded by this table. Same for the sign bit, which should be read after
/// the `Run`.
const TCOEF_TABLE: [Entry<Option<ShortTCoefficient>>; 207] = [
    Fork(8, 1), //x, slot 0
    Fork(2, 3), //1x, slot 1
    End(Some(Run {
        last: false,
        run: 0,
        level: 1,
    })), //10, slot 2
    Fork(4, 5), //11x, slot 3
    End(Some(Run {
        last: false,
        run: 1,
        level: 1,
    })), //110, slot 4
    Fork(6, 7), //111x, slot 5
    End(Some(Run {
        last: false,
        run: 2,
        level: 1,
    })), //1110, slot 6
    End(Some(Run {
        last: false,
        run: 0,
        level: 2,
    })), //1111, slot 7
    Fork(27, 9), //0x, slot 8
    Fork(15, 10), //01x, slot 9
    Fork(12, 11), //011x, slot 10
    End(Some(Run {
        last: true,
        run: 0,
        level: 1,
    })), //0111, slot 11
    Fork(13, 14), //0110x, slot 12
    End(Some(Run {
        last: false,
        run: 4,
        level: 1,
    })), //01100, slot 13
    End(Some(Run {
        last: false,
        run: 3,
        level: 1,
    })), //01101, slot 14
    Fork(16, 22), //010x, slot 15
    Fork(17, 20), //0100x, slot 16
    Fork(18, 19), //01000x, slot 17
    End(Some(Run {
        last: false,
        run: 9,
        level: 1,
    })), //010000, slot 18
    End(Some(Run {
        last: false,
        run: 8,
        level: 1,
    })), //010001, slot 19
    Fork(21, 22), //01001x, slot 20
    End(Some(Run {
        last: false,
        run: 7,
        level: 1,
    })), //010010, slot 21
    End(Some(Run {
        last: false,
        run: 6,
        level: 1,
    })), //010011, slot 22
    Fork(24, 23), //0101x, slot 22
    End(Some(Run {
        last: false,
        run: 5,
        level: 1,
    })), //01011, slot 23
    Fork(25, 26), //01010x, slot 24
    End(Some(Run {
        last: false,
        run: 1,
        level: 2,
    })), //010100, slot 25
    End(Some(Run {
        last: false,
        run: 0,
        level: 3,
    })), //010101, slot 26
    Fork(51, 28), //00x, slot 27
    Fork(36, 29), //001x, slot 28
    Fork(30, 33), //0011x, slot 29
    Fork(31, 32), //00110x, slot 30
    End(Some(Run {
        last: true,
        run: 4,
        level: 1,
    })), //001100, slot 31
    End(Some(Run {
        last: true,
        run: 3,
        level: 1,
    })), //001101, slot 32
    Fork(34, 35), //00111x, slot 33
    End(Some(Run {
        last: true,
        run: 2,
        level: 1,
    })), //001110, slot 34
    End(Some(Run {
        last: true,
        run: 1,
        level: 1,
    })), //001111, slot 35
    Fork(37, 44), //0010x, slot 36
    Fork(38, 41), //00100x, slot 37
    Fork(39, 40), //001000x, slot 38
    End(Some(Run {
        last: true,
        run: 8,
        level: 1,
    })), //0010000, slot 39
    End(Some(Run {
        last: true,
        run: 7,
        level: 1,
    })), //0010001, slot 40
    Fork(42, 43), //001001x, slot 41
    End(Some(Run {
        last: true,
        run: 6,
        level: 1,
    })), //0010010, slot 42
    End(Some(Run {
        last: true,
        run: 5,
        level: 1,
    })), //0010011, slot 43
    Fork(45, 48), //00101x, slot 44
    Fork(46, 47), //001010x, slot 45
    End(Some(Run {
        last: false,
        run: 12,
        level: 1,
    })), //0010100, slot 46
    End(Some(Run {
        last: false,
        run: 11,
        level: 1,
    })), //0010101, slot 47
    Fork(49, 50), //001011x, slot 48
    End(Some(Run {
        last: false,
        run: 10,
        level: 1,
    })), //0010110, slot 49
    End(Some(Run {
        last: false,
        run: 0,
        level: 4,
    })), //0010111, slot 50
    Fork(89, 52), //000x, slot 51
    Fork(68, 53), //0001x, slot 52
    Fork(54, 61), //00011x, slot 53
    Fork(55, 58), //000110x, slot 54
    Fork(56, 57), //0001100x, slot 55
    End(Some(Run {
        last: true,
        run: 11,
        level: 1,
    })), //00011000, slot 56
    End(Some(Run {
        last: true,
        run: 10,
        level: 1,
    })), //00011001, slot 57
    Fork(59, 60), //0001101x, slot 58
    End(Some(Run {
        last: true,
        run: 9,
        level: 1,
    })), //00011010, slot 59
    End(Some(Run {
        last: false,
        run: 14,
        level: 1,
    })), //00011011, slot 60
    Fork(62, 65), //000111x, slot 61
    Fork(63, 64), //0001110x, slot 62
    End(Some(Run {
        last: false,
        run: 13,
        level: 1,
    })), //00011100, slot 63
    End(Some(Run {
        last: false,
        run: 2,
        level: 2,
    })), //00011101, slot 64
    Fork(66, 67), //0001111x, slot 65
    End(Some(Run {
        last: false,
        run: 1,
        level: 3,
    })), //00011110, slot 66
    End(Some(Run {
        last: false,
        run: 0,
        level: 5,
    })), //00011111, slot 67
    Fork(76, 69), //00010x, slot 68
    Fork(70, 73), //000101x, slot 69
    Fork(71, 72), //0001010x, slot 70
    End(Some(Run {
        last: true,
        run: 15,
        level: 1,
    })), //00010100, slot 71
    End(Some(Run {
        last: true,
        run: 14,
        level: 1,
    })), //00010101, slot 72
    Fork(74, 75), //0001011x, slot 73
    End(Some(Run {
        last: true,
        run: 13,
        level: 1,
    })), //00010110, slot 74
    End(Some(Run {
        last: true,
        run: 12,
        level: 1,
    })), //00010111, slot 75
    Fork(77, 84), //000100x, slot 76
    Fork(78, 81), //0001000x, slot 77
    Fork(79, 80), //00010000x, slot 78
    End(Some(Run {
        last: false,
        run: 16,
        level: 1,
    })), //000100000, slot 79
    End(Some(Run {
        last: false,
        run: 15,
        level: 1,
    })), //000100001, slot 80
    Fork(82, 83), //00010001x, slot 81
    End(Some(Run {
        last: false,
        run: 4,
        level: 2,
    })), //000100010, slot 82
    End(Some(Run {
        last: false,
        run: 3,
        level: 2,
    })), //000100011, slot 83
    Fork(85, 88), //0001001x, slot 84
    Fork(86, 87), //00010010x, slot 85
    End(Some(Run {
        last: false,
        run: 0,
        level: 7,
    })), //000100100, slot 86
    End(Some(Run {
        last: false,
        run: 0,
        level: 6,
    })), //000100101, slot 87
    End(Some(Run {
        last: true,
        run: 16,
        level: 1,
    })), //00010011x, slot 88
    Fork(123, 90), //0000x, slot 89
    Fork(91, 108), //00001x, slot 90
    Fork(92, 101), //000010x, slot 91
    Fork(93, 98), //0000100x, slot 92
    Fork(94, 97), //00001000x, slot 93
    Fork(95, 96), //000010000x, slot 94
    End(Some(Run {
        last: false,
        run: 0,
        level: 9,
    })), //0000100000, slot 95
    End(Some(Run {
        last: false,
        run: 0,
        level: 8,
    })), //0000100001, slot 96
    End(Some(Run {
        last: true,
        run: 24,
        level: 1,
    })), //000010001, slot 97
    Fork(99, 100), //00001001x, slot 98
    End(Some(Run {
        last: true,
        run: 23,
        level: 1,
    })), //000010010, slot 99
    End(Some(Run {
        last: true,
        run: 22,
        level: 1,
    })), //000010011, slot 100
    Fork(102, 105), //0000101x, slot 101
    Fork(103, 104), //00001010x, slot 102
    End(Some(Run {
        last: true,
        run: 21,
        level: 1,
    })), //000010100, slot 103
    End(Some(Run {
        last: true,
        run: 20,
        level: 1,
    })), //000010101, slot 104
    Fork(106, 107), //00001011x, slot 105
    End(Some(Run {
        last: true,
        run: 19,
        level: 1,
    })), //000010110, slot 106
    End(Some(Run {
        last: true,
        run: 18,
        level: 1,
    })), //000010111, slot 107
    Fork(109, 116), //000011x, slot 108
    Fork(110, 113), //0000110x, slot 109
    Fork(111, 112), //00001100x, slot 110
    End(Some(Run {
        last: true,
        run: 17,
        level: 1,
    })), //000011000, slot 111
    End(Some(Run {
        last: true,
        run: 0,
        level: 2,
    })), //000011001, slot 112
    Fork(114, 115), //00001101x, slot 113
    End(Some(Run {
        last: false,
        run: 22,
        level: 1,
    })), //000011010, slot 114
    End(Some(Run {
        last: false,
        run: 21,
        level: 1,
    })), //000011011, slot 115
    Fork(117, 120), //0000111x, slot 116
    Fork(118, 119), //00001110x, slot 117
    End(Some(Run {
        last: false,
        run: 20,
        level: 1,
    })), //000011100, slot 118
    End(Some(Run {
        last: false,
        run: 19,
        level: 1,
    })), //000011101, slot 119
    Fork(121, 122), //00001111x, slot 120
    End(Some(Run {
        last: false,
        run: 18,
        level: 1,
    })), //000011110, slot 121
    End(Some(Run {
        last: false,
        run: 17,
        level: 1,
    })), //000011111, slot 122
    Fork(173, 124), //00000x, slot 123
    Fork(126, 125), //000001x, slot 124
    End(Some(EscapeToLong)), //0000011, slot 125
    Fork(127, 142), //0000010x, slot 126
    Fork(128, 135), //00000100x, slot 127
    Fork(129, 132), //000001000x, slot 128
    Fork(130, 131), //0000010000x, slot 129
    End(Some(Run {
        last: false,
        run: 0,
        level: 12,
    })), //00000100000, slot 130
    End(Some(Run {
        last: false,
        run: 1,
        level: 5,
    })), //00000100001, slot 131
    Fork(133, 134), //0000010001x, slot 132
    End(Some(Run {
        last: false,
        run: 23,
        level: 1,
    })), //00000100010, slot 133
    End(Some(Run {
        last: false,
        run: 24,
        level: 1,
    })), //00000100011, slot 134
    Fork(136, 139), //000001001x, slot 135
    Fork(137, 138), //0000010010x, slot 136
    End(Some(Run {
        last: true,
        run: 29,
        level: 1,
    })), //00000100100, slot 137
    End(Some(Run {
        last: true,
        run: 30,
        level: 1,
    })), //00000100101, slot 138
    Fork(140, 141), //0000010011x, slot 139
    End(Some(Run {
        last: true,
        run: 31,
        level: 1,
    })), //00000100110, slot 140
    End(Some(Run {
        last: true,
        run: 32,
        level: 1,
    })), //00000100111, slot 141
    Fork(143, 158), //00000101x, slot 142
    Fork(144, 151), //000001010x, slot 143
    Fork(145, 148), //0000010100x, slot 144
    Fork(146, 147), //00000101000x, slot 145
    End(Some(Run {
        last: false,
        run: 1,
        level: 6,
    })), //000001010000, slot 146
    End(Some(Run {
        last: false,
        run: 2,
        level: 4,
    })), //000001010001, slot 147
    Fork(149, 150), //00000101001x, slot 148
    End(Some(Run {
        last: false,
        run: 4,
        level: 3,
    })), //000001010010, slot 149
    End(Some(Run {
        last: false,
        run: 5,
        level: 3,
    })), //000001010011, slot 150
    Fork(152, 155), //0000010101x, slot 151
    Fork(153, 154), //00000101010x, slot 152
    End(Some(Run {
        last: false,
        run: 6,
        level: 3,
    })), //000001010100, slot 153
    End(Some(Run {
        last: false,
        run: 10,
        level: 2,
    })), //000001010101, slot 154
    Fork(156, 157), //00000101011x, slot 155
    End(Some(Run {
        last: false,
        run: 25,
        level: 1,
    })), //000001010110, slot 156
    End(Some(Run {
        last: false,
        run: 26,
        level: 1,
    })), //000001010111, slot 157
    Fork(159, 166), //000001011x, slot 158
    Fork(160, 163), //0000010110x, slot 159
    Fork(161, 162), //00000101100x, slot 160
    End(Some(Run {
        last: true,
        run: 33,
        level: 1,
    })), //000001011000, slot 161
    End(Some(Run {
        last: true,
        run: 34,
        level: 1,
    })), //000001011001, slot 162
    Fork(164, 165), //00000101101x, slot 163
    End(Some(Run {
        last: true,
        run: 35,
        level: 1,
    })), //000001011010, slot 164
    End(Some(Run {
        last: true,
        run: 36,
        level: 1,
    })), //000001011011, slot 165
    Fork(167, 170), //0000010111x, slot 166
    Fork(168, 169), //00000101110x, slot 167
    End(Some(Run {
        last: true,
        run: 37,
        level: 1,
    })), //000001011100, slot 168
    End(Some(Run {
        last: true,
        run: 38,
        level: 1,
    })), //000001011101, slot 169
    Fork(171, 172), //00000101111x, slot 170
    End(Some(Run {
        last: true,
        run: 39,
        level: 1,
    })), //000001011110, slot 171
    End(Some(Run {
        last: true,
        run: 40,
        level: 1,
    })), //000001011111, slot 172
    Fork(189, 174), //000000x, slot 173
    Fork(175, 182), //0000001x, slot 174
    Fork(176, 179), //00000010x, slot 175
    Fork(177, 178), //000000100x, slot 176
    End(Some(Run {
        last: false,
        run: 9,
        level: 2,
    })), //0000001000, slot 177
    End(Some(Run {
        last: false,
        run: 8,
        level: 2,
    })), //0000001001, slot 178
    Fork(180, 181), //000000101x, slot 179
    End(Some(Run {
        last: false,
        run: 7,
        level: 2,
    })), //0000001010, slot 180
    End(Some(Run {
        last: false,
        run: 6,
        level: 2,
    })), //0000001011, slot 181
    Fork(183, 186), //00000011x, slot 182
    Fork(184, 185), //000000110x, slot 183
    End(Some(Run {
        last: false,
        run: 5,
        level: 2,
    })), //0000001100, slot 184
    End(Some(Run {
        last: false,
        run: 3,
        level: 3,
    })), //0000001101, slot 185
    Fork(187, 188), //000000111x, slot 186
    End(Some(Run {
        last: false,
        run: 2,
        level: 3,
    })), //0000001110, slot 187
    End(Some(Run {
        last: false,
        run: 1,
        level: 4,
    })), //0000001111, slot 188
    Fork(197, 190), //0000000x, slot 189
    Fork(191, 194), //00000001x, slot 190
    Fork(192, 193), //000000010x, slot 191
    End(Some(Run {
        last: true,
        run: 28,
        level: 1,
    })), //0000000100, slot 192
    End(Some(Run {
        last: true,
        run: 27,
        level: 1,
    })), //0000000101, slot 193
    Fork(195, 196), //000000011x, slot 194
    End(Some(Run {
        last: true,
        run: 26,
        level: 1,
    })), //0000000110, slot 195
    End(Some(Run {
        last: true,
        run: 25,
        level: 1,
    })), //0000000111, slot 196
    Fork(205, 198), //00000000x, slot 197
    Fork(199, 202), //000000001x, slot 198
    Fork(200, 201), //0000000010x, slot 199
    End(Some(Run {
        last: true,
        run: 1,
        level: 2,
    })), //00000000100, slot 200
    End(Some(Run {
        last: true,
        run: 0,
        level: 3,
    })), //00000000101, slot 201
    Fork(203, 204), //0000000011x, slot 202
    End(Some(Run {
        last: false,
        run: 0,
        level: 11,
    })), //00000000110, slot 203
    End(Some(Run {
        last: false,
        run: 0,
        level: 10,
    })), //00000000111, slot 204
    End(None),  //000000000x, slot 205
];

/// Decode a block from the bitstream.
///
/// The `running_options` should be the set of currently in-force options
/// present on the currently-decoded picture. This is not entirely equivalent
/// to the current picture's option set as some options can carry forward from
/// picture to picture without being explicitly mentioned.
///
/// The `macroblock_type` should be the `MacroblockType` recovered from the
/// currently-decoded macroblock.
fn decode_block<R>(
    reader: &mut H263Reader<R>,
    running_options: PictureOption,
    macroblock_type: MacroblockType,
) -> Result<Block>
where
    R: Read,
{
    reader.with_transaction(|reader| {
        let intradc = if macroblock_type.is_intra() {
            Some(IntraDC::from_u8(reader.read_u8()?).ok_or(Error::InvalidBitstream)?)
        } else {
            None
        };

        let mut tcoef = Vec::new();
        loop {
            let short_tcoef = reader.read_vlc(&TCOEF_TABLE[..])?;

            let last = match short_tcoef.ok_or(Error::InvalidBitstream)? {
                EscapeToLong => {
                    let last = reader.read_bits::<u8>(1)? == 0;
                    let run: u8 = reader.read_bits(6)?;
                    let level = reader.read_u8()?;

                    if level == 0 {
                        return Err(Error::InvalidBitstream);
                    }

                    //TODO: Modified Quantization (Annex T)
                    if level == 0x80 {
                        if running_options.contains(PictureOption::ModifiedQuantization) {
                            return Err(Error::UnimplementedDecoding);
                        } else {
                            return Err(Error::InvalidBitstream);
                        }
                    }

                    tcoef.push(TCoefficient {
                        is_short: false,
                        run,
                        level: level as i8,
                    });

                    last
                }
                Run { last, run, level } => {
                    let sign: u8 = reader.read_bits(1)?;
                    if sign == 0 {
                        tcoef.push(TCoefficient {
                            is_short: true,
                            run,
                            level: level as i8,
                        })
                    } else {
                        tcoef.push(TCoefficient {
                            is_short: true,
                            run,
                            level: -(level as i8),
                        })
                    }

                    last
                }
            };

            if last {
                break;
            }
        }

        Ok(Block { intradc, tcoef })
    })
}
