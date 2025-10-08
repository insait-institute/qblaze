// SPDX-License-Identifier: Apache-2.0
use crate::{Complex, Qubit, QubitSet};
use crate::bitset::BitSet;

#[derive(Copy, Clone)]
#[repr(C)]
pub struct Element<const N: usize> {
    pub bits: BitSet<N>,
    pub val: Complex,
}

#[derive(Default)]
pub struct Stat {
    pub ops: usize,
    pub shuffle: bool,
    pub modified: QubitSet,
}

impl Stat {
    #[inline]
    pub(super) fn write(&mut self, q: Qubit) {
        self.modified.insert(q);
    }
}

trait Gate: 'static + Sized + Send + Sync {
    type Prep<const N: usize>: Copy + Sized + Send + Sync;
    fn prepare<const N: usize>(&self) -> Self::Prep<N>;
    fn perform<const N: usize>(arg: Self::Prep<N>, elt: &mut Element<N>);
}

mod gates;

mod compiler;
pub use compiler::Compiled;

mod queue;
pub use queue::Queue;
