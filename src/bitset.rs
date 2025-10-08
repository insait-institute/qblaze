// SPDX-License-Identifier: Apache-2.0
use std::{hint, ops};

pub type Word = u64;
pub const BITSET_SHIFT: u8 = 6;
pub const BITSET_WORD: usize = 1 << BITSET_SHIFT;

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct BitIndex<const N: usize>(u32);

impl<const N: usize> BitIndex<N> {
    const CHK: () = {
        assert!(N <= 1usize << 25);
    };

    #[inline]
    pub fn new(v: usize) -> Self {
        assert!(v < N << 6);
        #[allow(clippy::let_unit_value)]
        let _ = Self::CHK;
        Self(v as u32)
    }

    #[inline]
    pub fn new_u32(v: u32) -> Self {
        assert!(v < (N as u32) << 6);
        #[allow(clippy::let_unit_value)]
        let _ = Self::CHK;
        Self(v)
    }

    #[inline]
    pub unsafe fn new_unchecked(v: usize) -> Self {
        unsafe {
            hint::assert_unchecked(v < (N << 6));
        }
        Self(v as u32)
    }

    #[inline(always)]
    pub fn get_u32(self) -> u32 {
        unsafe {
            hint::assert_unchecked(self.0 < (N as u32) << BITSET_SHIFT);
        }
        self.0
    }

    #[inline(always)]
    pub fn get(self) -> usize {
        let v = self.0 as usize;
        unsafe {
            hint::assert_unchecked(v < N << BITSET_SHIFT);
        }
        v
    }

    #[inline(always)]
    fn split(self) -> (usize, u8) {
        let word = (self.0 >> BITSET_SHIFT) as usize;
        let bit = (self.0 as u8) & (BITSET_WORD - 1) as u8;
        unsafe {
            hint::assert_unchecked(word < N);
        }
        (word, bit)
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BitSet<const N: usize>([Word; N]);

impl<const N: usize> Default for BitSet<N> {
    #[inline(always)]
    fn default() -> Self {
        Self([0; N])
    }
}

impl<const N: usize> BitSet<N> {
    #[inline(always)]
    pub fn get(&self, i: BitIndex<N>) -> bool {
        let i = i.get();
        let m = (1 as Word) << (i % BITSET_WORD);
        *unsafe { self.0.get_unchecked(i / BITSET_WORD) } & m != 0
    }

    #[inline(always)]
    pub fn set(&mut self, i: BitIndex<N>, v: bool) {
        let (i_word, i_shift) = i.split();
        let m = (1 as Word) << i_shift;
        let p = unsafe { self.0.get_unchecked_mut(i_word) };
        if v {
            *p |= m;
        } else {
            *p &= !m;
        }
    }

    #[inline(always)]
    pub fn toggle(&mut self, i: BitIndex<N>) {
        let (i_word, i_shift) = i.split();
        let w = (1 as Word) << i_shift;
        *unsafe { self.0.get_unchecked_mut(i_word) } ^= w;
    }
}

impl<const N: usize> ops::Not for BitSet<N> {
    type Output = BitSet<N>;

    #[inline]
    fn not(mut self) -> Self {
        for i in 0..N {
            self.0[i] = !self.0[i];
        }
        self
    }
}

impl<const N: usize> ops::BitAnd for BitSet<N> {
    type Output = BitSet<N>;

    #[inline]
    fn bitand(mut self, other: Self) -> Self {
        for i in 0..N {
            self.0[i] &= other.0[i];
        }
        self
    }
}

impl<const N: usize> ops::BitAndAssign for BitSet<N> {
    #[inline]
    fn bitand_assign(&mut self, other: Self) {
        for i in 0..N {
            self.0[i] &= other.0[i];
        }
    }
}

impl<const N: usize> ops::BitOr for BitSet<N> {
    type Output = BitSet<N>;

    #[inline]
    fn bitor(mut self, other: Self) -> Self {
        for i in 0..N {
            self.0[i] |= other.0[i];
        }
        self
    }
}

impl<const N: usize> ops::BitOrAssign for BitSet<N> {
    #[inline]
    fn bitor_assign(&mut self, other: Self) {
        for i in 0..N {
            self.0[i] |= other.0[i];
        }
    }
}

impl<const N: usize> ops::BitXor for BitSet<N> {
    type Output = BitSet<N>;

    #[inline]
    fn bitxor(mut self, other: Self) -> Self {
        for i in 0..N {
            self.0[i] ^= other.0[i];
        }
        self
    }
}

impl<const N: usize> ops::BitXorAssign for BitSet<N> {
    #[inline]
    fn bitxor_assign(&mut self, other: Self) {
        for i in 0..N {
            self.0[i] ^= other.0[i];
        }
    }
}

impl<const N: usize> FromIterator<BitIndex<N>> for BitSet<N> {
    fn from_iter<I: IntoIterator<Item = BitIndex<N>>>(it: I) -> Self {
        let mut r = BitSet::default();
        for i in it {
            r.set(i, true);
        }
        r
    }
}
