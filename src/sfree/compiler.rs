// SPDX-License-Identifier: Apache-2.0
use super::{Gate, Element};
use crate::{arena, bitset::BitIndex, shamelessly_stolen_from_libstd::partition_by, Qubit};
use std::{marker::PhantomData, ptr};

#[derive(Copy, Clone)]
pub struct Compiled<'a, const N: usize> {
    marker: PhantomData<fn(&'a u8)>,
    func: unsafe fn(usize, &mut [Element<N>]),
    arg: usize,
}

unsafe impl<'a, const N: usize> Send for Compiled<'a, N>{}

impl<'a, const N: usize> Compiled<'a, N> {
    pub fn perform(self, elts: &mut [Element<N>]) {
        unsafe { (self.func)(self.arg, elts) }
    }

    pub fn from_func<F: Copy + Sync + Fn(&mut [Element<N>])>(alloc: &'a arena::Arena, f: F) -> Self {
        if size_of::<F>() > size_of::<usize>() {
            return Self::from_large_func(alloc, f);
        }

        unsafe fn invoke<const N: usize, F: Sync + Fn(&mut [Element<N>])>(arg: usize, data: &mut [Element<N>]) {
            let f = unsafe { (&raw const arg as *const F).read() };
            f(data)
        }
        let mut arg = 0usize;
        unsafe {
            ptr::copy_nonoverlapping(&raw const f, (&raw mut arg) as *mut F, 1);
        }
        Self {
            marker: PhantomData,
            func: invoke::<N, F>,
            arg,
        }
    }

    pub fn from_large_func<F: Sync + Fn(&mut [Element<N>])>(alloc: &'a arena::Arena, f: F) -> Self {
        unsafe fn invoke<const N: usize, F: Sync + Fn(&mut [Element<N>])>(arg: usize, data: &mut [Element<N>]) {
            let f = unsafe { (arg as *const F).read() };
            f(data)
        }
        let mem: *const F = alloc.alloc(f);
        Self {
            marker: PhantomData,
            func: invoke::<N, F>,
            arg: mem as usize,
        }
    }

    pub(super) fn from_gate<G: Gate>(alloc: &'a arena::Arena, g: G) -> Self {
        let prep = g.prepare::<N>();
        Self::from_func(alloc, #[inline(always)] move |elts| {
            for elt in elts {
                G::perform(prep, elt);
            }
        })
    }

    pub(super) fn from_seq(alloc: &'a arena::Arena, seq: Vec<Self>) -> Self {
        if seq.len() == 1 {
            return seq.into_iter().next().unwrap();
        }
        let seq: &arena::Seq<Self> = alloc.alloc_vec(seq);
        Self::from_func(alloc, #[inline(always)] move |elts| {
            for gate in &seq[..] {
                gate.perform(elts);
            }
        })
    }

    pub(super) fn from_pseq(alloc: &'a arena::Arena, ctl: Qubit, seq0: Vec<Self>, seq1: Vec<Self>) -> Self {
        let ctl: BitIndex<N> = ctl.into();
        let seq0: &[Self] = alloc.alloc_vec(seq0);
        let seq1: &[Self] = alloc.alloc_vec(seq1);
        Self::from_func(alloc, #[inline(always)] move |elts| {
            let i = partition_by(
                elts,
                #[inline(always)]
                move |elt| !elt.bits.get(ctl),
            );
            let (elts0, elts1) = elts.split_at_mut(i);
            if !elts0.is_empty() {
                for g in seq0 {
                    g.perform(elts0);
                }
            }
            if !elts1.is_empty() {
                for g in seq1 {
                    g.perform(elts1);
                }
            }
        })
    }
}
