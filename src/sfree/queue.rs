// SPDX-License-Identifier: Apache-2.0
use super::{gates, Compiled, Gate, Stat};
use crate::{arena, bitset::BitSet, Angle, Control, ControlItem, Qubit, QubitSet, U3};
use std::mem;

const THRES_PART_FAIL: isize = -2;
const THRES_PART_DO: usize = 5;

type OpSeq = Vec<(Control, Op)>;
type OpIter = <OpSeq as IntoIterator>::IntoIter;

#[derive(Default)]
pub struct Queue {
    ops: OpSeq,
    reads: QubitSet,
    writes: QubitSet,
}

impl Queue {
    pub fn reads(&self, q: Qubit) -> bool {
        self.reads.contains(q)
    }

    pub fn writes(&self, q: Qubit) -> bool {
        self.writes.contains(q)
    }

    pub fn max_qubit(&self) -> Qubit {
        let mut r = Qubit::from_index(0);
        if let Some(q) = self.reads.iter().max() {
            r = r.max(q);
        }
        if let Some(q) = self.writes.iter().max() {
            r = r.max(q);
        }
        r
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    pub fn x(&mut self, q: Qubit) {
        self.writes.insert(q);
        self.ops.push((Control::always(), Op::X(q)));
    }

    pub fn ctl_phase(&mut self, ctl: Control, a: Angle) {
        self.reads.extend(ctl.iter().map(ControlItem::qubit));
        self.ops.push((ctl, Op::Gphase(a)));
    }

    pub fn ctl_x(&mut self, ctl: Control, q: Qubit) {
        self.reads.extend(ctl.iter().map(ControlItem::qubit));
        self.writes.insert(q);
        self.ops.push((ctl, Op::X(q)));
    }

    pub fn ctl_swap(&mut self, ctl: Control, q1: Qubit, q2: Qubit) {
        self.reads.extend(ctl.iter().map(ControlItem::qubit));
        self.reads.insert(q1);
        self.reads.insert(q2);
        self.writes.insert(q1);
        self.writes.insert(q2);
        self.ops.push((ctl, Op::Swap(q1, q2)));
    }

    pub fn blocks(&mut self, q: Qubit, gate: U3, prec: u8) -> bool {
        let (check_read, check_write) = match gate.to_rz_rx(prec) {
            None => (true, true),
            Some((rz, rx)) => (rx.round(prec) != Angle::ZERO, rz.round(prec) != Angle::ZERO),
        };
        if check_read && self.reads(q) {
            return true;
        }
        if check_write && self.writes(q) {
            return true;
        }
        false
    }

    #[inline]
    pub fn compile<'a, const N: usize>(&mut self, alloc: &'a arena::Arena, st: &mut Stat, prec: u8) -> Option<Compiled<'a, N>> {
        self.reads.clear();
        self.writes.clear();
        let ops = mem::take(&mut self.ops);
        let (mut inner, toggled, _gphase) = compile_seq(alloc, st, ops, QubitSet::new(), prec);
        push_toggle_many(alloc, st, &mut inner, Control::always(), toggled);
        if inner.is_empty() {
            return None;
        }
        Some(Compiled::from_seq(alloc, inner))
    }
}

#[derive(Clone)]
enum Op {
    Gphase(Angle),
    X(Qubit),
    Swap(Qubit, Qubit),
}

impl Op {
    fn writes(&self, t: Qubit) -> bool {
        match *self {
            Op::Gphase(_) => false,
            Op::X(q) => q == t,
            Op::Swap(q1, q2) => q1 == t || q2 == t,
        }
    }
}

fn eval_control(q: Qubit, ops: &[(Control, Op)]) -> (isize, usize) {
    let mut best_score = 0;
    let mut best_len = 0;

    let mut cur_score = 2;
    for (i, (ctl, op)) in ops.iter().enumerate() {
        if op.writes(q) {
            break;
        }
        if ctl.uses(q) {
            cur_score += 2;
        } else {
            cur_score -= 1;
        }
        if cur_score > best_score {
            best_score = cur_score;
            best_len = i + 1;
        }
        if cur_score <= THRES_PART_FAIL {
            break;
        }
    }

    (best_score, best_len)
}

fn select_control(it: impl IntoIterator<Item = Qubit>, ops: &[(Control, Op)]) -> (Qubit, usize) {
    let mut best_score = 0;
    let mut best_res = (Qubit::from_index(0), 0);
    for q in it {
        let (score, len) = eval_control(q, ops);
        if score > best_score {
            best_score = score;
            best_res = (q, len);
        }
    }
    best_res
}

fn try_partition(ctl0: &Control, op0: &Op, ops: &mut OpIter) -> Option<(Qubit, OpSeq, OpSeq)> {
    let pending = ops.as_slice();
    if pending.is_empty() {
        return None;
    }
    let (ctl_q, ctl_l) = select_control(ctl0.iter().map(ControlItem::qubit), pending);
    if ctl_l < THRES_PART_DO {
        return None;
    }

    let mut if0 = vec![];
    let mut if1 = vec![];

    match ctl0.without_qubit(ctl_q) {
        None => panic!(),
        Some((false, ctl)) => if0.push((ctl, op0.clone())),
        Some((true, ctl)) => if1.push((ctl, op0.clone())),
    }

    for _ in 0..ctl_l {
        let (ctl, op) = ops.next().unwrap();
        match ctl.without_qubit(ctl_q) {
            None => {
                if0.push((ctl.clone(), op.clone()));
                if1.push((ctl, op));
            }
            Some((false, ctl)) => if0.push((ctl, op)),
            Some((true, ctl)) => if1.push((ctl, op)),
        }
    }
    Some((ctl_q, if0, if1))
}

fn make_controlled<'a, const N: usize, G: Gate>(alloc: &'a arena::Arena, ctl: Control, gate: G) -> Compiled<'a, N> {
    match *ctl.get() {
        [] => Compiled::from_gate(alloc, gate),
        [ctl] => match ctl.value() {
            true => Compiled::from_gate(alloc, gates::SinglyControlled::<true, G>(ctl.qubit(), gate)),
            false => Compiled::from_gate(alloc, gates::SinglyControlled::<false, G>(ctl.qubit(), gate)),
        },
        _ => Compiled::from_gate(alloc, gates::MultiControlled::<G>(ctl, gate)),
    }
}

fn push_phase<'a, const N: usize>(alloc: &'a arena::Arena, st: &mut Stat, ops: &mut Vec<Compiled<'a, N>>, ctl: Control, a: Angle, prec: u8) {
    let op = match a.round(prec) {
        Angle::ZERO => return,
        Angle::PI_1_4 => make_controlled(alloc, ctl, gates::Pt),
        Angle::PI_1_2 => make_controlled(alloc, ctl, gates::Ps),
        Angle::PI_3_4 => make_controlled(alloc, ctl, gates::Pt3),
        Angle::PI => make_controlled(alloc, ctl, gates::Pz),
        Angle::PI_5_4 => make_controlled(alloc, ctl, gates::Pt5),
        Angle::PI_3_2 => make_controlled(alloc, ctl, gates::Psdg),
        Angle::PI_7_4 => make_controlled(alloc, ctl, gates::Ptdg),
        _ => make_controlled(alloc, ctl, gates::P(a)),
    };
    st.ops += 1;
    ops.push(op);
}

fn push_toggle_one<'a, const N: usize>(alloc: &'a arena::Arena, st: &mut Stat, ops: &mut Vec<Compiled<'a, N>>, ctl: Control, q: Qubit) {
    st.ops += 1;
    st.write(q);
    ops.push(make_controlled(alloc, ctl, gates::X(q)));
}

fn push_toggle_many<'a, const N: usize>(alloc: &'a arena::Arena, st: &mut Stat, ops: &mut Vec<Compiled<'a, N>>, ctl: Control, to_toggle: QubitSet) {
    let n = to_toggle.len();
    if n == 0 {
        return;
    }
    if n >= size_of::<BitSet<N>>() / 8 {
        st.ops += 1;
        for q in to_toggle.iter() {
            st.write(q);
        }
        ops.push(make_controlled(alloc, ctl, gates::MultiX(to_toggle)));
        return;
    }
    for q in to_toggle.iter() {
        push_toggle_one(alloc, st, ops, ctl.clone(), q);
    }
}

fn push_swap<'a, const N: usize>(alloc: &'a arena::Arena, st: &mut Stat, ops: &mut Vec<Compiled<'a, N>>, ctl: Control, q1: Qubit, q2: Qubit) {
    st.ops += 1;
    st.write(q1);
    st.write(q2);
    ops.push(make_controlled(alloc, ctl, gates::Swap(q1, q2)));
}

fn compile_seq<'a, const N: usize>(alloc: &'a arena::Arena, st: &mut Stat, ops: OpSeq, mut toggled: QubitSet, prec: u8) -> (Vec<Compiled<'a, N>>, QubitSet, Angle) {
    let mut ops = ops.into_iter();
    let mut gphase = Angle::ZERO;
    let mut r = Vec::<Compiled<N>>::new();

    while let Some((mut ctl, op)) = ops.next() {
        if let Some((ctl_q, mut if0, mut if1)) = try_partition(&ctl, &op, &mut ops) {
            if toggled.contains(ctl_q) {
                mem::swap(&mut if0, &mut if1);
            }
            let (mut if0, mut toggle0, phase0) = compile_seq::<N>(alloc, st, if0, toggled.clone(), prec);
            let (mut if1, mut toggle1, phase1) = compile_seq::<N>(alloc, st, if1, toggled, prec);

            toggled = toggle0.intersection(&toggle1);
            for q in toggled.iter() {
                toggle0.remove(q);
                toggle1.remove(q);
            }
            push_toggle_many(alloc, st, &mut if0, Control::always(), toggle0);
            push_toggle_many(alloc, st, &mut if1, Control::always(), toggle1);

            let phase_diff = phase1 - phase0;
            if phase_diff.round(prec) == Angle::ZERO {
                gphase += Angle::mid(phase0, phase1);
                if if0.is_empty() && if1.is_empty() {
                    continue;
                }
            } else if (gphase + phase1).round(prec) == Angle::ZERO {
                gphase = Angle::ZERO;
                if if0.is_empty() && if1.is_empty() {
                    push_phase(alloc, st, &mut r, Control::single(ctl_q, false), -phase_diff, prec);
                    continue;
                }
                push_phase(alloc, st, &mut if0, Control::always(), -phase_diff, prec);
            } else {
                gphase += phase0;
                if if0.is_empty() && if1.is_empty() {
                    push_phase(alloc, st, &mut r, Control::single(ctl_q, true), phase_diff, prec);
                    continue;
                }
                push_phase(alloc, st, &mut if1, Control::always(), phase_diff, prec);
            }

            st.shuffle = true;
            r.push(Compiled::from_pseq(alloc, ctl_q, if0, if1));
            continue;
        }

        if ctl.is_always() {
            match op {
                Op::Gphase(a) => {
                    gphase += a;
                }
                Op::X(q) => {
                    if !toggled.remove(q) {
                        toggled.insert(q);
                    }
                }
                Op::Swap(q1, q2) => {
                    let has1 = toggled.contains(q1);
                    let has2 = toggled.contains(q2);
                    if has1 && !has2 {
                        toggled.remove(q1);
                        toggled.insert(q2);
                    } else if has2 && !has1 {
                        toggled.remove(q2);
                        toggled.insert(q1);
                    }
                    // TODO reorder swaps?
                    push_swap(alloc, st, &mut r, Control::always(), q1, q2);
                }
            }
            continue;
        }

        ctl.inplace_toggle_if(|q| toggled.contains(q));
        match op {
            Op::Gphase(a) => {
                push_phase(alloc, st, &mut r, ctl, a, prec);
            },
            Op::X(q) => {
                push_toggle_one(alloc, st, &mut r, ctl, q);
            },
            Op::Swap(q1, q2) => {
                let has1 = toggled.contains(q1);
                let has2 = toggled.contains(q2);
                if has1 != has2 {
                    let q = if has1 { q1 } else { q2 };
                    toggled.remove(q);
                    push_toggle_one(alloc, st, &mut r, Control::always(), q);
                }
                push_swap(alloc, st, &mut r, ctl, q1, q2);
            }
        }
    }
    (r, toggled, gphase)
}
