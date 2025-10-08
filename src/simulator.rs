// SPDX-License-Identifier: Apache-2.0
use crate::{sfree, statevector, Angle, Complex, Config, Context, Control, Qubit, U3, U4};
use std::{hash::BuildHasher, slice};
use std::collections::hash_map::{Entry, HashMap, RandomState};

const PREC: u8 = 32;
const N_SAMP: usize = 32;

pub struct Simulator {
    pub(crate) ctx: Context,
    // raw state vector
    state: Box<dyn statevector::Statevector>,
    // queued sfree gates
    sfree_queue: sfree::Queue,
    // queued single-qubit gates
    qubits: HashMap<Qubit, QubitState>,
}

#[derive(Default)]
struct QubitState {
    // queued single-qubit gate
    pending: U3,

    // do we need to check the qubit for destructive interference
    maybe_destructive: bool,

    // during gate application: do we need to flush
    must_flush: bool,

    // during gate application: we reordered the gate past the pending operation,
    // so don't flush and mark as dirty instead
    is_reordered: bool,
}

impl Simulator {
    fn new_with_state(ctx: Context, state: Box<dyn statevector::Statevector>) -> Self {
        Self {
            ctx,
            state,
            sfree_queue: sfree::Queue::default(),
            qubits: HashMap::default(),
        }
    }

    /// Create a new simulator instance.
    pub fn new(cfg: Config) -> Self {
        assert!(cfg.threads > 0);
        assert!(cfg.chunk_size > 0);
        let ctx = Context::new(&cfg);
        let state = statevector::new(cfg.max_qubit);
        Self::new_with_state(ctx, state)
    }

    /// Clone a simulator instance. The instance must be flushed.
    pub fn clone_flushed(&self) -> Self {
        assert!(self.sfree_queue.is_empty());
        assert!(self.qubits.is_empty());
        let mut ctx = Context::new(&self.ctx.config());
        let state = self.state.dyn_clone(&mut ctx);
        Self::new_with_state(ctx, state)
    }

    /// Check if an error has occurred.
    pub fn is_error(&self) -> bool {
        self.state.is_empty()
    }

    /// Return the maximum qubit index that can be used without resizing.
    pub fn max_qubit(&self) -> Qubit {
        self.state.max_qubit()
    }

    /// Apply a U3 gate.
    pub fn u3(&mut self, q: Qubit, gate: U3) {
        match self.qubits.entry(q) {
            Entry::Occupied(mut ent) => {
                let st = ent.get_mut();
                st.pending = U3::compose(st.pending, gate);
                st.maybe_destructive = true;
            }
            Entry::Vacant(ent) => {
                ent.insert(QubitState {
                    pending: gate,
                    maybe_destructive: true,
                    must_flush: false,
                    is_reordered: false,
                });
            }
        }
    }

    /// Apply a multiply controlled X gate.
    pub fn ctl_x(&mut self, ctl: &Control, q: Qubit) {
        if ctl.is_always() {
            return self.u3(q, U3::X);
        }
        assert!(!ctl.uses(q));

        trace_match!("preflushing {:?} for X", q);
        let PreflushX { is_h } = self._preflush(q);

        if let Some(q_inv) = is_h {
            let (ctl, q2) = self._preflush_ctl_h(ctl);
            let ctl = ctl.with_qubit(q, !q_inv);
            self._flush(false);
            if let Some(q2) = q2 {
                trace_match!("push ctl_x on {:?}, retarget to {:?} if {:?}", q, q2, ctl);
                self.sfree_queue.ctl_x(ctl, q2);
            } else {
                trace_match!("push ctl_x on {:?}, as ctl_z {:?}", q, ctl);
                self.sfree_queue.ctl_phase(ctl, Angle::PI);
            }
        } else {
            let ctl = self._preflush_ctl(ctl);
            self._flush(false);
            trace_match!("push ctl_x on {:?}", q);
            self.sfree_queue.ctl_x(ctl, q);
        }
    }

    /// Apply a multiply controlled swap gate.
    pub fn ctl_swap(&mut self, ctl: &Control, q1: Qubit, q2: Qubit) {
        assert!(!ctl.uses(q1));
        assert!(!ctl.uses(q2));
        assert!(q1 != q2);
        let ctl = self._preflush_ctl(ctl);
        trace_match!("preflushing {:?} for swap", q1);
        let PreflushI = self._preflush(q1);
        trace_match!("preflushing {:?} for swap", q2);
        let PreflushI = self._preflush(q2);
        self._flush(false);
        trace_match!("push ctl_swap on {:?}, {:?}", q1, q2);
        self.sfree_queue.ctl_swap(ctl, q1, q2);
    }

    /// Apply a multiply controlled phase gate.
    pub fn ctl_phase(&mut self, ctl: &Control, a: Angle) {
        match ctl.get() {
            [] => {}
            [c] => {
                self.u3(c.qubit(), U3::rz(if c.value() { a } else { -a }));
            }
            _ => {
                let ctl = if a.round(PREC) == Angle::PI {
                    let (ctl, q) = self._preflush_ctl_h(ctl);
                    if let Some(q) = q {
                        self._flush(false);
                        trace_match!("push ctl_z as ctl_x on {:?}", q);
                        self.sfree_queue.ctl_x(ctl, q);
                        return;
                    }
                    ctl
                } else {
                    self._preflush_ctl(ctl)
                };
                self._flush(false);
                self.sfree_queue.ctl_phase(ctl, a);
            }
        }
    }

    fn premeasure(&mut self, q: Qubit) -> (bool, f64, f64) {
        trace_match!("preflushing {:?} for measure", q);
        let PreflushCtl { invert } = self._preflush(q);
        self._flush(self.sfree_queue.writes(q));

        if q > self.state.max_qubit() {
            return (invert, 1.0, 0.0);
        }

        let [r0, r1] = self.state.premeasure(&mut self.ctx, q);
        (invert, r0, r1)
    }

    /// Measure the target qubit.
    ///
    /// The state vector is probabilistically collapsed based on a random 64-bit input.
    ///
    /// Return the measured value, as well as the probability for 0 and for 1.
    /// On error, returns (false, NaN, NaN).
    pub fn measure(&mut self, q: Qubit, mut bias: u64) -> (bool, f64, f64) {
        let (invert, r0, r1) = self.premeasure(q);
        let sum = r0 + r1;
        if sum == 0.0 {
            return (false, f64::NAN, f64::NAN);
        }
        let r0n = r0 / sum;
        let r1n = r1 / sum;

        if invert {
            bias = !bias;
        }
        let v = if r0 <= r1 {
            bias >= ((r0n * 2.0f64.powi(64)) as u64)
        } else {
            !bias < ((r1n * 2.0f64.powi(64)) as u64)
        };

        // Skip measurement if it's a no-op.
        // Use exact equality because we delete empty states anyway.
        if (if v { r0 } else { r1 }) != 0.0 {
            self.state.measure(&mut self.ctx, q, v, if v { r1 } else { r0 }.sqrt());
        }

        if invert {
            (!v, r1n, r0n)
        } else {
            (v, r0n, r1n)
        }
    }

    /// Compute the measurement probabilities for the target qubit without measuring it.
    pub fn qubit_probs(&mut self, q: Qubit) -> (f64, f64) {
        let (invert, r0, r1) = self.premeasure(q);
        let sum = r0 + r1;
        let r0n = r0 / sum;
        let r1n = r1 / sum;
        if invert {
            (r1n, r0n)
        } else {
            (r0n, r1n)
        }
    }

    fn _preflush_ctl(&mut self, ctl: &Control) -> Control {
        ctl.iter().map(|c| {
            trace_match!("preflushing {:?} for control", c.qubit());
            let PreflushCtl { invert } = self._preflush(c.qubit());
            if invert {
                c.invert()
            } else {
                c
            }
        }).collect()
    }

    fn _preflush_ctl_h(&mut self, ctl: &Control) -> (Control, Option<Qubit>) {
        let mut hadamard = None;
        let ctl = ctl.iter().flat_map(|c| {
            trace_match!("preflushing {:?} for control+H", c.qubit());
            if hadamard.is_some() || !c.value() {
                // We don't want to deal with inverted controls.
                let PreflushCtl { invert } = self._preflush(c.qubit());
                return Some(if invert {
                    c.invert()
                } else {
                    c
                });
            }
            match self._preflush(c.qubit()) {
                PreflushCtlH::Normal => Some(c),
                PreflushCtlH::Invert => Some(c.invert()),
                PreflushCtlH::Hadamard => {
                    hadamard = Some(c.qubit());
                    None
                }
            }
        }).collect();
        (ctl, hadamard)
    }

    fn _preflush<R: Preflush>(&mut self, q: Qubit) -> R {
        let Entry::Occupied(mut ent) = self.qubits.entry(q) else {
            return R::from_id();
        };
        let st = ent.get_mut();
        let gate = st.pending;

        if let Some((is_x, rz)) = gate.to_cx_rz(PREC) {
            if is_x {
                if let Some(r) = R::from_x_rz() {
                    trace_match!("gate on {:?}, matched as x; rz({:?})", q, rz);
                    return r;
                }
                trace_match!("gate on {:?}, flushed x", q);
                self.sfree_queue.x(q);
            }
            if rz.round(PREC) != Angle::ZERO {
                if let Some(r) = R::from_rz() {
                    trace_match!("gate on {:?}, matched as rz({:?})", q, rz);
                    return r;
                }
                trace_match!("gate on {:?}, flushed rz({:?}), matched as noop", q, rz);
                self.sfree_queue.ctl_phase(Control::single(q, true), rz);
            } else {
                trace_match!("gate on {:?}, matched as noop", q);
            }
            ent.remove();
            return R::from_id();
        }

        debug_assert!(!gate.is_sfree(PREC));
        if st.maybe_destructive && !self.sfree_queue.blocks(q, gate, PREC) {
            if Self::maybe_eager_flush(&mut self.ctx, &mut *self.state, q, gate) {
                ent.remove();
                return R::from_id();
            }
            st.maybe_destructive = false;
        }

        if let Some((rz, rx)) = gate.to_rz_rx(PREC) {
            if let Some(r) = R::from_rx() {
                if rz.round(PREC) != Angle::ZERO {
                    // flush the rz part, commute past the rx part
                    trace_match!("gate on {:?}, flushed rz({:?}), matched as rx({:?})", q, rz, rx);
                    self.sfree_queue.ctl_phase(Control::single(q, true), rz);
                    st.is_reordered = true;
                    st.pending = U3::rx(rx);
                    debug_assert!(!st.pending.is_sfree(PREC));
                } else {
                    // commutes with current op
                    trace_match!("gate on {:?}, matched as rx({:?})", q, rx);
                }
                return r;
            }
        }

        if let Some((rz1, rz2)) = gate.to_rz_h_rz(PREC) {
            if let Some(r) = R::from_h_rz() {
                st.is_reordered = true;
                if rz1.round(PREC) != Angle::ZERO {
                    trace_match!("gate on {:?}, flushed as rz({:?}), matched h; rz({:?})", q, rz1, rz2);
                    self.sfree_queue.ctl_phase(Control::single(q, true), rz1);
                    st.pending = U3::compose(U3::H, U3::rz(rz2));
                } else {
                    trace_match!("gate on {:?}, matched h; rz({:?})", q, rz2);
                }
                return r;
            }
            let rz2r = rz2.round(PREC);
            if rz2r == Angle::ZERO {
                if let Some(r) = R::from_h_rx() {
                    trace_match!("gate on {:?}, move before h; rx({:?})", q, rz1);
                    st.is_reordered = true;
                    return r;
                }
            }
            if rz2r == Angle::PI {
                if let Some(r) = R::from_x_h_rx() {
                    trace_match!("gate on {:?}, move before x; h; rx({:?})", q, -rz1);
                    st.is_reordered = true;
                    return r;
                }
            }
        }

        trace_match!("gate on {:?} needs flush", q);
        st.must_flush = true;
        R::from_id()
    }

    #[inline]
    fn ensure_qubit(ctx: &mut Context, state: &mut Box<dyn statevector::Statevector>, q: Qubit) {
        if q <= state.max_qubit() {
            return;
        }
        perf!(ctx["grow"] {
            let mut new_state = statevector::new(q);
            let slices = state.export();
            new_state.import(ctx, slices);
            *state = new_state;
        });
    }

    fn maybe_eager_flush(ctx: &mut Context, state: &mut dyn statevector::Statevector, q: Qubit, gate: U3) -> bool {
        if gate.is_sfree(PREC) {
            return false;
        }
        if q > state.max_qubit() {
            return false;
        }
        let pre_len = state.len();
        if pre_len <= 32 * N_SAMP {
            return false;
        }

        let gate4 = U4::from(gate);
        let rng = RandomState::new();

        let mut delta = 0isize;
        for i in 0..N_SAMP {
            delta += state.collapse_delta(q, &gate4, rng.hash_one(i));
        }
        // eprintln!("[debug] sampled {:?}; delta={}/{}", q, delta, N_SAMP);
        if delta > -(N_SAMP as isize / 4) {
            return false;
        }

        let post_len;
        perf!(ctx["state-u4-eager", pre_len, post_len] {
            state.perform_u4(ctx, q, &gate4);
            post_len = state.len();
        });

        true
    }

    fn _flush(&mut self, mut also_sfree: bool) {
        if self.sfree_queue.is_empty() {
            also_sfree = false;
        } else if !also_sfree {
            for (q, st) in self.qubits.iter() {
                if !st.must_flush {
                    continue;
                }
                if self.sfree_queue.blocks(*q, st.pending, PREC) {
                    also_sfree = true;
                    break;
                }
            }
        }

        self.qubits.retain(|q, st| {
            if st.must_flush || st.is_reordered {
                return true;
            }
            let q = *q;
            let gate = st.pending;
            if self.sfree_queue.blocks(q, gate, PREC) {
                // Flusing the sfree queue may make it destructive.
                st.maybe_destructive = true;
                return true;
            }
            if !st.maybe_destructive {
                // Already checked.
                return true;
            }
            if Self::maybe_eager_flush(&mut self.ctx, &mut *self.state, q, gate) {
                return false;
            }
            st.maybe_destructive = false;
            true
        });

        // Ensure we have enough qubits.
        let mut max_q = if also_sfree {
            self.sfree_queue.max_qubit()
        } else {
            Qubit::from_index(0)
        };
        for (q, st) in self.qubits.iter() {
            if !st.must_flush {
                continue;
            }
            if *q > max_q {
                max_q = *q;
            }
        }
        Self::ensure_qubit(&mut self.ctx, &mut self.state, max_q);

        if also_sfree {
            let ctx = &mut self.ctx;
            let ops = self.sfree_queue.len();
            perf!(ctx["state-sfree", self.state.len(), ops] {
                self.state.perform_sfree(ctx, &mut self.sfree_queue, PREC);
            });

            // Try eager flush on the remaining qubits.
            self.qubits.retain(|q, st| {
                if st.is_reordered || !st.maybe_destructive {
                    return true;
                }
                let q = *q;
                let gate = st.pending;
                if Self::maybe_eager_flush(&mut self.ctx, &mut *self.state, q, gate) {
                    return false;
                }
                st.maybe_destructive = false;
                true
            });
        }

        self.qubits.retain(|q, st| {
            if !st.must_flush {
                st.is_reordered = false;
                return true;
            }
            debug_assert!(!st.is_reordered);
            let q = *q;
            trace_run!("u3 {:?}", st.pending);
            let gate = U4::from(st.pending);
            let ctx = &mut self.ctx;
            let n0 = self.state.len();
            perf!(ctx["state-u4", n0, self.state.len()] {
                self.state.perform_u4(ctx, q, &gate);
            });
            false
        });
    }

    /// Flush the gate queue.
    pub fn flush(&mut self) {
        let qubits: Vec<Qubit> = self.qubits.iter_mut().map(|(q, _)| *q).collect();
        for q in qubits {
            trace_match!("preflushing {:?} by force", q);
            let PreflushI = self._preflush(q);
        }
        self._flush(true);
    }

    /// Free unused memory.
    pub fn trim(&mut self) {
        self.state.trim();
    }

    /// Invoke a callback for each element of the state vector. Elements where qubits past the 64th 
    /// are set are skipped.
    pub fn iter<'a>(&'a self) -> Iter<'a> {
        let slice = self.state.export();
        assert!(slice.stride % size_of::<u64>() == 0);
        assert!(slice.bits_size >= size_of::<u64>());
        assert!(slice.bits_size % size_of::<u64>() == 0);
        assert!(unsafe { slice.ptr.add(slice.bits_offset) as *mut u64 }.is_aligned());
        Iter { slice }
    }
}

pub struct Iter<'a> {
    slice: statevector::ExportSlice<'a>,
}

impl<'a> Iterator for Iter<'a> {
    type Item = (&'a [u64], Complex);

    fn next(&mut self) -> Option<Self::Item> {
        if self.slice.len == 0 {
            return None;
        }
        let p = self.slice.ptr;
        unsafe {
            let pbits = p.add(self.slice.bits_offset) as *mut u64;
            let pval = p.add(self.slice.value_offset) as *mut Complex;
            let words = self.slice.bits_size / size_of::<u64>();
            self.slice.ptr = p.add(self.slice.stride);
            self.slice.len -= 1;
            Some((slice::from_raw_parts::<u64>(pbits, words), pval.read()))
        }
    }
}

trait Preflush: Sized {
    fn from_id() -> Self;

    #[inline(always)]
    fn from_rx() -> Option<Self> {
        None
    }
    #[inline(always)]
    fn from_rz() -> Option<Self> {
        None
    }
    #[inline(always)]
    fn from_x_rz() -> Option<Self> {
        None
    }
    #[inline(always)]
    fn from_h_rz() -> Option<Self> {
        None
    }
    #[inline(always)]
    fn from_h_rx() -> Option<Self> {
        None
    }
    #[inline(always)]
    fn from_x_h_rx() -> Option<Self> {
        None
    }
}

#[allow(dead_code)] // https://www.youtube.com/watch?v=Jdf5EXo6I68
struct PreflushCtl {
    invert: bool,
}

impl Preflush for PreflushCtl {
    fn from_id() -> Self {
        Self { invert: false }
    }
    fn from_rz() -> Option<Self> {
        Some(Self { invert: false })
    }
    fn from_x_rz() -> Option<Self> {
        Some(Self { invert: true })
    }
}

#[allow(dead_code)] // https://www.youtube.com/watch?v=Jdf5EXo6I68
enum PreflushCtlH {
    Normal,
    Invert,
    Hadamard,
}

impl Preflush for PreflushCtlH {
    fn from_id() -> Self {
        Self::Normal
    }
    fn from_rz() -> Option<Self> {
        Some(Self::Normal)
    }
    fn from_x_rz() -> Option<Self> {
        Some(Self::Invert)
    }
    fn from_h_rz() -> Option<Self> {
        Some(Self::Hadamard)
    }
}

#[allow(dead_code)] // https://www.youtube.com/watch?v=Jdf5EXo6I68
struct PreflushConst;

impl Preflush for PreflushConst {
    fn from_id() -> Self {
        PreflushConst
    }
    fn from_rz() -> Option<Self> {
        Some(PreflushConst)
    }
}

#[allow(dead_code)] // https://www.youtube.com/watch?v=Jdf5EXo6I68
struct PreflushI;

impl Preflush for PreflushI {
    fn from_id() -> Self {
        PreflushI
    }
}

#[allow(dead_code)] // https://www.youtube.com/watch?v=Jdf5EXo6I68
struct PreflushX { is_h: Option<bool> }

impl Preflush for PreflushX {
    fn from_id() -> Self {
        PreflushX { is_h: None }
    }
    fn from_rx() -> Option<Self> {
        Some(PreflushX { is_h: None })
    }
    fn from_h_rx() -> Option<Self> {
        Some(PreflushX { is_h: Some(false) })
    }
    fn from_x_h_rx() -> Option<Self> {
        Some(PreflushX { is_h: Some(true) })
    }
}
