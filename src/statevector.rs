// SPDX-License-Identifier: Apache-2.0
use std::{sync::atomic::{AtomicU64, Ordering}, f64, marker::PhantomData, mem, ptr};

use crate::bitset::{BitIndex, BitSet, BITSET_WORD};
use crate::buffer::{Buf, BufWriter, MemoryError, RawBuf, WriteChunk};
use crate::Context;
use crate::{Complex, Qubit, U4};
use crate::sfree;
use crate::arena;
use crate::shamelessly_stolen_from_libstd::par_quicksort;
use crate::threading;
use crate::util;

#[derive(Copy, Clone)]
pub struct ExportSlice<'a> {
    marker: PhantomData<&'a [u8]>,
    pub ptr: *const u8,
    pub len: usize,
    pub stride: usize,
    pub bits_offset: usize,
    pub bits_size: usize,
    pub value_offset: usize,
    partitioned_by: Option<Qubit>,
    partitioned_at: usize,
}

unsafe impl Send for ExportSlice<'_> {}
unsafe impl Sync for ExportSlice<'_> {}

pub trait Statevector: Send + Sync {
    fn max_qubit(&self) -> Qubit;
    fn is_empty(&self) -> bool;
    fn len(&self) -> usize;
    fn collapse_delta(&self, q: Qubit, gate: &U4, i: u64) -> isize;
    fn premeasure(&self, ctx: &mut Context, q: Qubit) -> [f64; 2];
    fn measure(&mut self, ctx: &mut Context, q: Qubit, v: bool, scale: f64);
    fn perform_u4(&mut self, ctx: &mut Context, q: Qubit, gate: &U4);
    fn perform_sfree(&mut self, ctx: &mut Context, op: &mut sfree::Queue, prec: u8);
    fn export<'a>(&'a self) -> ExportSlice<'a>;
    fn import(&mut self, ctx: &mut Context, data: ExportSlice);
    fn trim(&mut self);
    fn dyn_clone(&self, ctx: &mut Context) -> Box<dyn Statevector>;
}

struct StatevectorImpl<const N: usize> {
    partitioned_by: Option<BitIndex<N>>,
    state: Buf<sfree::Element<N>, 2>,
    old_buf: RawBuf<sfree::Element<N>>,
}

impl<const N: usize> StatevectorImpl<N> {
    #[inline]
    fn new() -> Self {
        let state = match Buf::new_uninit([1, 0]) {
            Ok(mut state) => {
                state[0][0].write(sfree::Element {
                    bits: BitSet::default(),
                    val: Complex::ONE,
                });
                unsafe { state.assume_init() }
            }
            Err(MemoryError) => Buf::default(),
        };
        Self {
            partitioned_by: Some(BitIndex::new(0)),
            state,
            old_buf: RawBuf::empty(),
        }
    }

    fn clear(&mut self) {
        self.state = Buf::default();
        self.old_buf = RawBuf::empty();
    }

    fn set_state(&mut self, mut state: Buf<sfree::Element<N>, 2>) {
        state.trim_unused();
        mem::swap(&mut self.state, &mut state);
        if state.capacity_bytes() > self.old_buf.capacity_bytes() {
            self.old_buf = state.into_raw();
        }
    }

    fn lookup_random(&self, i: u64) -> Option<sfree::Element<N>> {
        let n = self.len();
        if n == 0 {
            return None;
        }
        let i = (i % (n as u64)) as usize;
        if i < self.state[0].len() {
            Some(self.state[0][i])
        } else {
            Some(self.state[1][i - self.state[0].len()])
        }
    }

    fn lookup(&self, key: &BitSet<N>) -> Option<Complex> {
        // The partitioning may be inconsistent, and this is not a hot path.
        for part in self.state.iter() {
            if let Ok(j) = part.binary_search_by(#[inline(always)] |st_j| st_j.bits.cmp(key)) {
                return Some(part[j].val)
            }
        }
        None
    }

    fn for_common(&self, ctx: &Context, max_chunk_size: usize) -> (bool, usize) {
        let n = self.state.total_len() * mem::size_of::<sfree::Element<N>>();

        if n < ctx.multithreading_threshold {
            return (false, 1);
        };

        let thr = ctx.pool.num_threads();
        if thr <= 1 {
            return (false, n.div_ceil(max_chunk_size));
        }
        if n <= ctx.work_item_min_size * thr {
            return (true, n.div_ceil(ctx.work_item_min_size));
        }

        let stride = max_chunk_size.saturating_mul(thr);
        let mut chunks_per_thread = n.div_ceil(stride);
        if chunks_per_thread <= 1 {
            return (true, thr);
        }
        if thr <= 1 {
            chunks_per_thread = 1;
        }

        (true, thr * chunks_per_thread)
    }

    /// Returns (use pool, target chunk count)
    fn for_merge(&self, ctx: &Context) -> (bool, usize) {
        let (use_pool, num_chunks) = self.for_common(ctx, ctx.work_item_max_size);
        // eprintln!("merge chunks, {} -> st={}, n={}", self.state.total_len(), ctx.pool.num_threads() <= 1, num_chunks);
        (use_pool, num_chunks)
    }

    fn for_chunks(&self, ctx: &Context, max_chunk_size: usize) -> (bool, [usize; 2]) {
        let (use_pool, num_chunks) = self.for_common(ctx, max_chunk_size);

        let n0 = self.state[0].len();
        let n1 = self.state[1].len();
        let n = n0 + n1;
        if num_chunks <= 2 {
            return (false, [n, n]);
        }

        let mut div = num_chunks;
        if n0 == 0 || n1 == 0 {
            let c = n.div_ceil(div);
            return (use_pool, [c, c]);
        }

        let mut k0;
        let mut k1;
        loop {
            let chunk = n.div_ceil(div);
            k0 = n0.div_ceil(chunk);
            k1 = n1.div_ceil(chunk);
            if k0 + k1 <= num_chunks {
                break;
            }
            div -= 1;
        }

        (use_pool, [n0.div_ceil(k0), n1.div_ceil(k1)])
    }

    fn for_small_chunks(&self, ctx: &Context) -> (bool, [usize; 2]) {
        self.for_chunks(ctx, ctx.chunk_size)
    }

    fn for_large_chunks(&self, ctx: &Context) -> (bool, [usize; 2]) {
        self.for_chunks(ctx, ctx.work_item_max_size)
    }

    fn for_sort(&self, ctx: &Context) -> bool {
        let n = self.state.total_len() * mem::size_of::<sfree::Element<N>>();
        if n <= ctx.multithreading_threshold {
            return false;
        }
        true
    }

    #[inline]
    fn set_partition(&mut self, ctx: &mut Context, qi: BitIndex<N>) {
        if self.partitioned_by == Some(qi) {
            return;
        }
        self.partitioned_by = Some(qi);
        self.force_repartition(ctx, qi, false)
    }

    fn force_repartition(&mut self, ctx: &mut Context, qi: BitIndex<N>, is_fixup: bool) {
        let (use_pool, count) = self.for_merge(ctx);
        let [part0, part1] = self.state.slices();
        let subchunks = perf!(ctx["repart-plan", part0.len(), part1.len()] {
            Self::do_subchunks(if is_fixup { Some(qi) } else { None }, count, part0, part1)
        });

        let subchunk_len: Vec<[usize; 2]> = perf!(ctx["repart-count", part0.len(), part1.len(), subchunks.len()] {
            let pool = ctx.pool.maybe_borrow(use_pool);
            pool.map(subchunks.iter(), #[inline(always)] move |(v0, v1)| {
                Self::do_repartition_count(qi, v0, v1)
            })
        });

        let Ok(mut w) = BufWriter::<_, 2>::reserve_chunks(
            mem::take(&mut self.old_buf),
            |i| subchunk_len.iter().map(move |p| p[i]),
        ) else {
            return self.clear();
        };
        let [out0, out1] = w.start();
        mem::drop(subchunk_len);

        perf!(ctx["repart-do", part0.len(), part1.len(), w.slice_len(0), w.slice_len(1)] {
            let pool = ctx.pool.maybe_borrow(use_pool);
            pool.scope(|sc| {
                if is_fixup {
                    sc.for_each(
                        subchunks.iter().copied().zip(out0).zip(out1),
                        #[inline(always)] move |(((v0, v1), o0), o1)| {
                            Self::do_repartition_fixup(qi, v0, v1, o0, o1)
                        },
                    );
                } else {
                    sc.for_each(
                        subchunks.iter().copied().zip(out0).zip(out1),
                        #[inline(always)] move |(((v0, v1), o0), o1)| {
                            Self::do_repartition_write(qi, v0, v1, o0, o1)
                        },
                    );
                }
            });
        });

        self.set_state(unsafe { w.finish() });
    }

    #[inline(never)]
    fn do_subchunks<'a, 'b>(
        qi: Option<BitIndex<N>>,
        count: usize,
        part0: &'a [sfree::Element<N>],
        part1: &'b [sfree::Element<N>],
    ) -> Vec<(&'a [sfree::Element<N>], &'b [sfree::Element<N>])> {
        let mut mask = !BitSet::default();
        if let Some(qi) = qi {
            mask.toggle(qi);
        }
        util::merge_chunk_pairs(part0, part1, count, #[inline(always)] move |v: &sfree::Element<N>| v.bits & mask)
    }

    #[inline(never)]
    fn do_repartition_count(
        qi: BitIndex<N>,
        chunk0: &[sfree::Element<N>],
        chunk1: &[sfree::Element<N>],
    ) -> [usize; 2] {
        let mut r = 0usize;
        for elt in chunk0.iter() {
            r += elt.bits.get(qi) as usize;
        }
        for elt in chunk1.iter() {
            r += elt.bits.get(qi) as usize;
        }
        [chunk0.len() + chunk1.len() - r, r]
    }

    #[inline(never)]
    fn do_repartition_write(
        qi: BitIndex<N>,
        chunk0: &[sfree::Element<N>],
        chunk1: &[sfree::Element<N>],
        out0: WriteChunk<sfree::Element<N>>,
        out1: WriteChunk<sfree::Element<N>>,
    ) {
        let mut out = [out0, out1];
        util::merge_distinct(
            chunk0,
            chunk1,
            #[inline(always)] move |elt: &sfree::Element<N>| elt.bits,
            #[inline(always)] move |elt: sfree::Element<N>| {
                unsafe { out[elt.bits.get(qi) as usize].push_unchecked(elt) };
            },
        );
    }

    #[inline(never)]
    fn do_repartition_fixup(
        qi: BitIndex<N>,
        chunk0: &[sfree::Element<N>],
        chunk1: &[sfree::Element<N>],
        out0: WriteChunk<sfree::Element<N>>,
        out1: WriteChunk<sfree::Element<N>>,
    ) {
        let mut out = [out0, out1];
        let mut mask = !BitSet::default();
        mask.toggle(qi);
        util::merge_distinct(
            chunk0,
            chunk1,
            #[inline(always)] move |elt: &sfree::Element<N>| elt.bits & mask,
            #[inline(always)] move |elt: sfree::Element<N>| {
                unsafe { out[elt.bits.get(qi) as usize].push_unchecked(elt) };
            },
        );
    }

    #[inline(never)]
    fn do_u4_count(
        qi: BitIndex<N>,
        gate: &U4,
        chunk0: &[sfree::Element<N>],
        chunk1: &[sfree::Element<N>],
    ) -> [usize; 2] {
        let mut mask = !BitSet::default();
        mask.toggle(qi);
        let mut n0 = 0usize;
        let mut n1 = 0usize;
        util::merge_eq(
            chunk0,
            chunk1,
            #[inline(always)] move |v| v.bits & mask,
            #[inline(always)] |v| match v {
                util::MergeEq::Lhs(_) => {
                    n0 += 1;
                    n1 += 1;
                }
                util::MergeEq::Rhs(_) => {
                    n0 += 1;
                    n1 += 1;
                }
                util::MergeEq::Both(v0, v1) => {
                    // Keep exactly in sync with the merging code below, needed for memory
                    // safety.
                    let (mut r0a, mut r1a) = gate.apply(false, v0.val);
                    let (r1b, r0b) = gate.apply(true, v1.val);
                    if r0a.accumulate(r0b) {
                        n0 += 1;
                    }
                    if r1a.accumulate(r1b) {
                        n1 += 1;
                    }
                }
            }
        );
        [n0, n1]
    }

    #[inline(never)]
    fn do_u4_write(
        qi: BitIndex<N>,
        gate: &U4,
        chunk0: &[sfree::Element<N>],
        chunk1: &[sfree::Element<N>],
        mut out0: WriteChunk<sfree::Element<N>>,
        mut out1: WriteChunk<sfree::Element<N>>,
    ) {
        let mut mask = !BitSet::default();
        mask.toggle(qi);
        util::merge_eq(
            chunk0,
            chunk1,
            #[inline(always)] move |v| v.bits & mask,
            #[inline(always)] move |v| match v {
                util::MergeEq::Lhs(v0) => {
                    let (p0, p1) = gate.apply(false, v0.val);
                    let r0 = sfree::Element { bits: v0.bits, val: p0 };
                    let mut r1 = sfree::Element { bits: v0.bits, val: p1 };
                    r1.bits.toggle(qi);
                    unsafe { out0.push_unchecked(r0) };
                    unsafe { out1.push_unchecked(r1) };
                }
                util::MergeEq::Rhs(v1) => {
                    let (p1, p0) = gate.apply(true, v1.val);
                    let mut r0 = sfree::Element { bits: v1.bits, val: p0 };
                    let r1 = sfree::Element { bits: v1.bits, val: p1 };
                    r0.bits.toggle(qi);
                    unsafe { out0.push_unchecked(r0) };
                    unsafe { out1.push_unchecked(r1) };
                }
                util::MergeEq::Both(v0, v1) => {
                    // Keep exactly in sync with the counting code above, needed for memory
                    // safety.
                    let (mut r0a, mut r1a) = gate.apply(false, v0.val);
                    let (r1b, r0b) = gate.apply(true, v1.val);
                    if r0a.accumulate(r0b) {
                        unsafe { out0.push_unchecked(sfree::Element {
                            bits: v0.bits,
                            val: r0a,
                        }) };
                    }
                    if r1a.accumulate(r1b) {
                        unsafe { out1.push_unchecked(sfree::Element {
                            bits: v1.bits,
                            val: r1a,
                        }) };
                    }
                }
            },
        );
    }

    #[inline(never)]
    fn do_premeasure(
        r: &[AtomicU64; 2],
        qi: BitIndex<N>,
        chunk: &[sfree::Element<N>],
    ) {
        let mut p = [0.0; 2];
        for elt in chunk.iter() {
            p[elt.bits.get(qi) as usize] += elt.val.norm2();
        }
        for (rv, v) in r.iter().zip(p.iter()) {
            rv.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |rv| {
                Some((f64::from_bits(rv) + *v).to_bits())
            }).unwrap();
        }
    }

    #[inline(never)]
    fn do_measure_count(
        qi: BitIndex<N>,
        v: bool,
        chunk: &[sfree::Element<N>],
    ) -> usize {
        let mut n = 0;
        for elt in chunk.iter() {
            n += elt.bits.get(qi) as usize;
        }
        if !v {
            n = chunk.len() - n;
        }
        n
    }

    #[inline(never)]
    fn do_measure_copy(
        qi: BitIndex<N>,
        v: bool,
        scale: f64,
        chunk: &[sfree::Element<N>],
        mut out: WriteChunk<sfree::Element<N>>,
    ) {
        for elt in chunk.iter() {
            if elt.bits.get(qi) != v {
                continue;
            }
            let mut elt = *elt;
            elt.val /= scale;
            unsafe { out.push_unchecked(elt) };
        }
    }

    #[inline(never)]
    fn do_sfree_apply(
        op: sfree::Compiled<N>,
        sort_chunks: bool,
        chunk: &mut [sfree::Element<N>],
    ) {
        op.perform(chunk);
        if sort_chunks {
            Self::do_sort(threading::Scope::single_threaded(), chunk)
        }
    }

    #[inline(never)]
    fn do_sfree_single(
        op: sfree::Compiled<N>,
        qi: BitIndex<N>,
        sort_chunks: bool,
        chunk: &mut [sfree::Element<N>],
    ) {
        op.perform(chunk);
        if sort_chunks {
            let mut mask = !BitSet::default();
            mask.toggle(qi);
            chunk.sort_unstable_by_key(#[inline(always)] move |s| s.bits & mask);
        }
    }

    #[inline(never)]
    fn do_sort<'a>(sc: threading::Scope<'a>, part: &'a mut [sfree::Element<N>]) {
        par_quicksort(sc, part, #[inline(always)] |v1, v2| v1.bits < v2.bits)
    }

    fn normalize(&mut self, ctx: &mut Context, scale: f64) {
        if (scale - 1.0).abs() < Complex::EPS {
            return;
        }

        let (use_pool, csize) = self.for_large_chunks(ctx);
        perf!(ctx["measure-normalize", self.len()] {
            let pool = ctx.pool.maybe_borrow(use_pool);
            let state = self.state.slices_mut();
            pool.scope(|sc| {
                for (s, c) in state.into_iter().zip(csize) {
                    sc.for_each(s.chunks_mut(c), move |chunk: &mut [sfree::Element<N>]| {
                        for elt in chunk {
                            elt.val /= scale;
                        }
                    });
                }
            });
        });
    }
}

impl<const N: usize> Statevector for StatevectorImpl<N> {
    fn max_qubit(&self) -> Qubit {
        Qubit::from_index(BITSET_WORD * N - 1)
    }

    #[inline(always)]
    fn is_empty(&self) -> bool {
        self.state.total_len() == 0
    }

    #[inline(always)]
    fn len(&self) -> usize {
        self.state.total_len()
    }

    fn collapse_delta(&self, q: Qubit, gate: &U4, i: u64) -> isize {
        let Some(st_i) = self.lookup_random(i) else {
            return 0;
        };
        let mut key = st_i.bits;
        let qi = BitIndex::<N>::from(q);
        let b = key.get(qi);
        key.toggle(qi);
        let Some(v_j) = self.lookup(&key) else {
            return 2;
        };
        let v_i = st_i.val;
        let (mut ap00, mut ap01) = gate.apply(b, v_i);
        let (ap11, ap10) = gate.apply(!b, v_j);
        let mut delta = 0;
        if !ap00.accumulate(ap10) {
            delta -= 1;
        }
        if !ap01.accumulate(ap11) {
            delta -= 1;
        }
        delta
    }

    fn premeasure(&self, ctx: &mut Context, q: Qubit) -> [f64; 2] {
        let qi = BitIndex::<N>::from(q);

        let (use_pool, csize) = self.for_large_chunks(ctx);
        perf!(ctx["measure-sum", self.len()] {
            let pool = ctx.pool.maybe_borrow(use_pool);
            let r = [const { AtomicU64::new(0) }; 2];
            let rr = &r;
            let state = self.state.slices();
            pool.scope(move |sc| {
                for (s, c) in state.into_iter().zip(csize) {
                    sc.for_each(
                        s.chunks(c),
                        #[inline(always)] move |chunk| {
                            Self::do_premeasure(rr, qi, chunk)
                        },
                    );
                }
            });
            r.map(|v| f64::from_bits(v.into_inner()))
        })
    }

    fn measure(&mut self, ctx: &mut Context, q: Qubit, v: bool, scale: f64) {
        let qi = BitIndex::<N>::from(q);

        if self.partitioned_by == Some(qi) {
            let n0 = self.state[0].len();
            let n1 = self.state[1].len();
            unsafe {
                if v {
                    let p = self.state.as_mut_ptr();
                    p.add(n0).copy_to(p, n1);
                    self.state.set_len([0, n1]);
                } else {
                    self.state.set_len([n0, 0]);
                }
            }
            return self.normalize(ctx, scale);
        }

        let all_len: Vec<usize>;
        let part_len: [&[usize]; 2];

        let (use_pool, csize) = self.for_large_chunks(ctx);
        perf!(ctx["measure-count", self.len()] {
            let pool = ctx.pool.maybe_borrow(use_pool);
            let [s0, s1] = self.state.slices();
            let c0 = s0.chunks(csize[0]);
            let c1 = s1.chunks(csize[1]);
            let mid = c0.len();
            all_len = pool.map(c0.chain(c1), #[inline(always)] move |chunk| {
                Self::do_measure_count(qi, v, chunk)
            });
            part_len = [
                &all_len[..mid],
                &all_len[mid..],
            ];
        });

        if all_len.iter().copied().sum::<usize>() == self.state.total_len() {
            // should be a probability-1.0 measurement
            return self.normalize(ctx, scale);
        }

        let Ok(mut w) = BufWriter::reserve_chunks(
            mem::take(&mut self.old_buf),
            |i| part_len[i].iter().copied(),
        ) else {
            return self.clear();
        };
        let out = w.start();

        let state = self.state.slices();
        perf!(ctx["measure-do", state[0].len(), state[1].len(), w.slice_len(0), w.slice_len(1)] {
            let pool = ctx.pool.maybe_borrow(use_pool);
            pool.scope(move |sc| {
                for ((s, c), o) in state.into_iter().zip(csize).zip(out) {
                    sc.for_each(
                        s.chunks(c).zip(o),
                        #[inline(always)] move |(chunk, out)| {
                            Self::do_measure_copy(qi, v, scale, chunk, out)
                        },
                    );
                }
            });
        });

        self.set_state(unsafe { w.finish() });
    }

    fn perform_u4(&mut self, ctx: &mut Context, q: Qubit, gate: &U4) {
        let qi = BitIndex::<N>::from(q);
        self.set_partition(ctx, qi);

        let (use_pool, count) = self.for_merge(ctx);
        let [part0, part1] = self.state.slices();
        let subchunks = perf!(ctx["u4-plan", part0.len(), part1.len()] {
            Self::do_subchunks(Some(qi), count, part0, part1)
        });

        let subchunk_len: Vec<[usize; 2]> = perf!(ctx["u4-prealloc", part0.len(), part1.len()] {
            let pool = ctx.pool.maybe_borrow(use_pool);
            pool.map(subchunks.iter(), #[inline(always)] move |(v0, v1)| {
                Self::do_u4_count(qi, gate, v0, v1)
            })
        });

        let Ok(mut w) = BufWriter::<_, 2>::reserve_chunks(
            mem::take(&mut self.old_buf),
            |i| subchunk_len.iter().map(move |p| p[i]),
        ) else {
            return self.clear();
        };
        let [out0, out1] = w.start();
        mem::drop(subchunk_len);

        perf!(ctx["u4-apply", part0.len(), part1.len(), w.slice_len(0), w.slice_len(1)] {
            let pool = ctx.pool.maybe_borrow(use_pool);
            pool.scope(move |sc| {
                sc.for_each(
                    subchunks.into_iter().zip(out0).zip(out1),
                    #[inline(always)] move |(((v0, v1), o0), o1)| {
                        Self::do_u4_write(qi, gate, v0, v1, o0, o1)
                    },
                );
            });
        });

        self.set_state(unsafe { w.finish() });
    }

    fn perform_sfree(&mut self, ctx: &mut Context, opq: &mut sfree::Queue, prec: u8) {
        let op;
        let arena = arena::Arena::new();
        let mut stat = sfree::Stat::default();
        let n0 = opq.len();
        perf!(ctx["sfree-compile", n0, stat.ops] {
            op = opq.compile(&arena, &mut stat, prec);
        });
        let Some(op) = op else {
            return;
        };

        if stat.modified.len() == 0 {
            let (use_pool, csize) = self.for_small_chunks(ctx);
            let sort_chunks = stat.shuffle;
            perf!(ctx["sfree-const", self.len(), stat.ops] {
                let pool = ctx.pool.maybe_borrow(use_pool);
                let state = self.state.slices_mut();
                pool.scope(move |sc| {
                    for (s, c) in state.into_iter().zip(csize) {
                        sc.for_each(
                            s.chunks_mut(c),
                            #[inline(always)] move |chunk| {
                                Self::do_sfree_apply(op, sort_chunks, chunk)
                            }
                        );
                    }
                });
            });
            return;
        }

        if stat.modified.len() == 1 {
            let qi = BitIndex::<N>::from(stat.modified.iter().next().unwrap());
            self.set_partition(ctx, qi);

            let (use_pool, csize) = self.for_small_chunks(ctx);
            let sort_chunks = stat.shuffle;
            perf!(ctx["sfree-single", self.len(), stat.ops] {
                let pool = ctx.pool.maybe_borrow(use_pool);
                let state = self.state.slices_mut();
                pool.scope(move |sc| {
                    for (s, c) in state.into_iter().zip(csize) {
                        sc.for_each(
                            s.chunks_mut(c),
                            #[inline(always)] move |chunk| {
                                Self::do_sfree_single(op, qi, sort_chunks, chunk)
                            }
                        );
                    }
                });
            });

            self.force_repartition(ctx, qi, true);
            return;
        }

        let (use_pool, csize) = self.for_small_chunks(ctx);
        perf!(ctx["sfree-apply", self.len(), stat.ops] {
            let pool = ctx.pool.maybe_borrow(use_pool);
            let state = self.state.slices_mut();
            pool.scope(move |sc| {
                for (s, c) in state.into_iter().zip(csize) {
                    sc.for_each(
                        s.chunks_mut(c),
                        #[inline(always)] move |chunk| {
                            Self::do_sfree_apply(op, false, chunk)
                        }
                    );
                }
            });
        });

        let use_pool = self.for_sort(ctx);
        let [part0, part1] = self.state.slices_mut();
        perf!(ctx["sfree-sort", part0.len(), part1.len(), stat.modified.len()] {
            let pool = ctx.pool.maybe_borrow(use_pool);
            let part0 = &mut *part0;
            let part1 = &mut *part1;
            pool.scope(move |sc| {
                sc.submit_one(move |sc| Self::do_sort(sc, part0));
                Self::do_sort(sc, part1);
            });
        });

        if let Some(qi) = self.partitioned_by {
            let q = Qubit::from(qi);
            if stat.modified.contains(q) {
                self.partitioned_by = None;
            }
        }
    }

    fn export<'a>(&'a self) -> ExportSlice<'a> {
        let stride = mem::size_of::<sfree::Element<N>>();
        let bits_offset = mem::offset_of!(sfree::Element<N>, bits);
        let bits_size = mem::size_of::<BitSet<N>>();
        let value_offset = mem::offset_of!(sfree::Element<N>, val);
        let s = self.state.single_slice();
        ExportSlice {
            marker: PhantomData,
            ptr: s.as_ptr() as *const u8,
            len: s.len(),
            stride,
            bits_offset,
            bits_size,
            value_offset,
            partitioned_by: self.partitioned_by.map(Qubit::from),
            partitioned_at: self.state[0].len(),
        }
    }

    fn import(&mut self, ctx: &mut Context, data: ExportSlice) {
        const CHUNK_SIZE: usize = 1 << 12;
        let Ok(mut buf) = Buf::<mem::MaybeUninit<sfree::Element<N>>, 1>::new_uninit([data.len]) else {
            return self.clear();
        };

        ctx.pool.borrow().scope(|sc| {
            sc.for_each(buf[0].chunks_mut(CHUNK_SIZE).enumerate(), move |(chunk_i, chunk)| {
                let data = data;
                let bits_size = data.bits_size.min(mem::size_of::<BitSet<N>>());
                for (i, out) in chunk.iter_mut().enumerate() {
                    let i = chunk_i * CHUNK_SIZE + i;
                    unsafe {
                        let p = data.ptr.add(i * data.stride);
                        let mut bits = BitSet::default();
                        ptr::copy::<u8>(p.add(data.bits_offset), (&mut bits) as *mut BitSet<N> as *mut u8, bits_size);
                        out.write(sfree::Element {
                            bits,
                            val: (p.add(data.value_offset) as *mut Complex).read(),
                        });
                    }
                }
            });
        });

        unsafe {
            self.state = buf.assume_init().extend_empty();
            self.state.set_len([data.partitioned_at, data.len - data.partitioned_at]);
        }
        self.partitioned_by = data.partitioned_by.map(BitIndex::from);
    }

    fn trim(&mut self) {
        let _ = mem::take(&mut self.old_buf);
    }

    fn dyn_clone(&self, ctx: &mut Context) -> Box<dyn Statevector> {
        let mut r = Box::new(Self::new());
        let data = self.export();
        r.import(ctx, data);
        r
    }
}

pub fn new(q: Qubit) -> Box<dyn Statevector> {
    let n = q.index();

    if n < 1 * BITSET_WORD {
        Box::new(StatevectorImpl::<1>::new())
    } else if n < 2 * BITSET_WORD {
        Box::new(StatevectorImpl::<2>::new())
    } else if n < 6 * BITSET_WORD {
        Box::new(StatevectorImpl::<6>::new())
    } else if n < 14 * BITSET_WORD {
        Box::new(StatevectorImpl::<14>::new())
    } else if n < 22 * BITSET_WORD {
        Box::new(StatevectorImpl::<22>::new())
    } else if n < 30 * BITSET_WORD {
        Box::new(StatevectorImpl::<30>::new())
    } else if n < 46 * BITSET_WORD {
        Box::new(StatevectorImpl::<46>::new())
    } else {
        const MAX_WORDS: usize = Qubit::MAX.index().div_ceil(64);
        Box::new(StatevectorImpl::<MAX_WORDS>::new())
    }
}
