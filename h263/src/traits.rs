//! Traits

use num_traits::{CheckedShl, Zero};
use std::ops::BitOr;

pub trait BitReadable: CheckedShl + BitOr<Self, Output = Self> + Zero + From<u8> {}

impl<T> BitReadable for T where T: CheckedShl + BitOr<Self, Output = Self> + Zero + From<u8> {}
