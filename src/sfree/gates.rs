// SPDX-License-Identifier: Apache-2.0
use super::{Element, Gate};
use crate::{bitset::{BitIndex, BitSet}, Angle, Complex, Control, Qubit, QubitSet};
use std::f64;

pub struct X(pub Qubit);

impl Gate for X {
    type Prep<const N: usize> = BitIndex<N>;

    fn prepare<const N: usize>(&self) -> Self::Prep<N> {
        BitIndex::from(self.0)
    }

    #[inline(always)]
    fn perform<const N: usize>(i: Self::Prep<N>, elt: &mut Element<N>) {
        elt.bits.toggle(i)
    }
}

pub struct Swap(pub Qubit, pub Qubit);

impl Gate for Swap {
    type Prep<const N: usize> = (BitIndex<N>, BitIndex<N>);

    fn prepare<const N: usize>(&self) -> Self::Prep<N> {
        (self.0.into(), self.1.into())
    }

    #[inline(always)]
    fn perform<const N: usize>((i0, i1): Self::Prep<N>, elt: &mut Element<N>) {
        if elt.bits.get(i0) != elt.bits.get(i1) {
            elt.bits.toggle(i0);
            elt.bits.toggle(i1);
        }
    }
}

pub struct P(pub Angle);

impl Gate for P {
    type Prep<const N: usize> = Complex;

    fn prepare<const N: usize>(&self) -> Self::Prep<N> {
        Complex::from_arg(self.0)
    }

    #[inline(always)]
    fn perform<const N: usize>(p: Self::Prep<N>, elt: &mut Element<N>) {
        elt.val *= p;
    }
}

pub struct Pz;

impl Gate for Pz {
    type Prep<const N: usize> = ();

    fn prepare<const N: usize>(&self) -> Self::Prep<N> {}

    #[inline(always)]
    fn perform<const N: usize>((): Self::Prep<N>, elt: &mut Element<N>) {
        elt.val = -elt.val;
    }
}

pub struct Ps;

impl Gate for Ps {
    type Prep<const N: usize> = ();

    fn prepare<const N: usize>(&self) -> Self::Prep<N> {}

    #[inline(always)]
    fn perform<const N: usize>((): Self::Prep<N>, elt: &mut Element<N>) {
        elt.val = elt.val.mul_i();
    }
}

pub struct Psdg;

impl Gate for Psdg {
    type Prep<const N: usize> = ();

    fn prepare<const N: usize>(&self) -> Self::Prep<N> {}

    #[inline(always)]
    fn perform<const N: usize>((): Self::Prep<N>, elt: &mut Element<N>) {
        elt.val = elt.val.div_i();
    }
}

pub struct Pt;

impl Gate for Pt {
    type Prep<const N: usize> = ();

    fn prepare<const N: usize>(&self) -> Self::Prep<N> {}

    #[inline(always)]
    fn perform<const N: usize>((): Self::Prep<N>, elt: &mut Element<N>) {
        let (re, im) = elt.val.to_cartesian();
        elt.val = Complex::from_cartesian(
            (re - im) * f64::consts::FRAC_1_SQRT_2,
            (re + im) * f64::consts::FRAC_1_SQRT_2,
        );
    }
}

pub struct Pt3;

impl Gate for Pt3 {
    type Prep<const N: usize> = ();

    fn prepare<const N: usize>(&self) -> Self::Prep<N> {}

    #[inline(always)]
    fn perform<const N: usize>((): Self::Prep<N>, elt: &mut Element<N>) {
        let (re, im) = elt.val.to_cartesian();
        elt.val = Complex::from_cartesian(
            -(re + im) * f64::consts::FRAC_1_SQRT_2,
            (re - im) * f64::consts::FRAC_1_SQRT_2,
        );
    }
}

pub struct Pt5;

impl Gate for Pt5 {
    type Prep<const N: usize> = ();

    fn prepare<const N: usize>(&self) -> Self::Prep<N> {}

    #[inline(always)]
    fn perform<const N: usize>((): Self::Prep<N>, elt: &mut Element<N>) {
        let (re, im) = elt.val.to_cartesian();
        elt.val = Complex::from_cartesian(
            (im - re) * f64::consts::FRAC_1_SQRT_2,
            -(re + im) * f64::consts::FRAC_1_SQRT_2,
        );
    }
}

pub struct Ptdg;

impl Gate for Ptdg {
    type Prep<const N: usize> = ();

    fn prepare<const N: usize>(&self) -> Self::Prep<N> {}

    #[inline(always)]
    fn perform<const N: usize>((): Self::Prep<N>, elt: &mut Element<N>) {
        let (re, im) = elt.val.to_cartesian();
        elt.val = Complex::from_cartesian(
            (re + im) * f64::consts::FRAC_1_SQRT_2,
            (im - re) * f64::consts::FRAC_1_SQRT_2,
        );
    }
}

pub struct MultiX(pub QubitSet);

impl Gate for MultiX {
    type Prep<const N: usize> = BitSet<N>;

    fn prepare<const N: usize>(&self) -> Self::Prep<N> {
        self.0.iter().map(BitIndex::from).collect()
    }

    #[inline(always)]
    fn perform<const N: usize>(mask: Self::Prep<N>, elt: &mut Element<N>) {
        elt.bits ^= mask;
    }
}

pub struct SinglyControlled<const V: bool, G: Gate>(pub Qubit, pub G);

impl<const V: bool, G: Gate> Gate for SinglyControlled<V, G> {
    type Prep<const N: usize> = (BitIndex<N>, G::Prep<N>);

    fn prepare<const N: usize>(&self) -> Self::Prep<N> {
        (BitIndex::from(self.0), self.1.prepare())
    }

    #[inline(always)]
    fn perform<const N: usize>((i, inner): Self::Prep<N>, elt: &mut Element<N>) {
        if elt.bits.get(i) != V {
            return;
        }
        G::perform(inner, elt)
    }
}

pub struct MultiControlled<G: Gate>(pub Control, pub G);

impl<G: Gate> Gate for MultiControlled<G> {
    type Prep<const N: usize> = (BitSet<N>, BitSet<N>, G::Prep<N>);

    fn prepare<const N: usize>(&self) -> Self::Prep<N> {
        let mask = self.0.iter().map(|ctl| ctl.qubit().into()).collect::<BitSet<N>>();
        let want = self.0.iter().filter_map(|ctl| {
            if ctl.value() {
                Some(ctl.qubit().into())
            } else {
                None
            }
        }).collect::<BitSet<N>>();
        (mask, want, self.1.prepare())
    }

    #[inline(always)]
    fn perform<const N: usize>((mask, want, inner): Self::Prep<N>, elt: &mut Element<N>) {
        if elt.bits & mask != want {
            return;
        }
        G::perform(inner, elt)
    }
}
