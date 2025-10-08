// SPDX-License-Identifier: Apache-2.0
use std::{f64, fmt, ops};

#[derive(Copy, Clone, PartialEq)]
#[repr(transparent)]
pub struct Angle(u64);

/// An angle in the range (-&pi;, &pi;].
impl Angle {
    pub const ZERO: Angle = Angle(0);
    pub const PI_1_4: Angle = Angle(0x2000000000000000); // T
    pub const PI_1_2: Angle = Angle(0x4000000000000000); // S
    pub const PI_3_4: Angle = Angle(0x6000000000000000); // S;T
    pub const PI: Angle = Angle(0x8000000000000000); // Z
    pub const PI_5_4: Angle = Angle(0xa000000000000000); // Tdg;Sdg
    pub const PI_3_2: Angle = Angle(0xc000000000000000); // Sdg
    pub const PI_7_4: Angle = Angle(0xe000000000000000); // Tdg

    pub fn from_radians(v: f64) -> Self {
        Self::from_turns(v / f64::consts::TAU)
    }

    pub fn from_turns(mut v: f64) -> Self {
        if v.is_nan() {
            panic!("NaN");
        }
        if v.is_infinite() {
            panic!("Inf");
        }
        let mut neg = false;
        if v.is_sign_negative() {
            neg = true;
            v = -v;
        }
        let base = 2.0f64.powi(64);
        v = (base * v.fract()).round_ties_even();
        assert!(0.0 <= v && v <= base);
        // `(base + v) also has a zero mantissa`
        let r = Angle((base + v).to_bits().wrapping_shl(64 - (f64::MANTISSA_DIGITS - 1)));
        if neg { -r } else { r }
    }

    pub fn from_atan2(mut y: f64, mut x: f64) -> Self {
        let flip_x = x.is_sign_negative();
        x = x.abs();
        let flip_xy = x < y.abs();
        if flip_xy {
            (x, y) = (y, x);
        }
        let mut r = Self::from_radians(f64::atan2(y, x));
        if flip_xy {
            r = Self::PI_1_2 - r;
        }
        if flip_x {
            r = Self::PI - r;
        }
        r
    }

    pub(crate) fn from_2atan2_pos(mut y: f64, mut x: f64) -> Self {
        debug_assert!(x >= 0.0);
        debug_assert!(y >= 0.0);
        let flip_xy = x < y;
        if flip_xy {
            (x, y) = (y, x);
        }
        let mut r = Self::from_radians(2.0 * f64::atan2(y, x));
        if flip_xy {
            r = Self::PI - r;
        }
        r
    }

    pub(crate) fn round(self, bits: u8) -> Self {
        if bits >= 64 {
            return self;
        }
        if bits == 0 {
            return Angle::ZERO;
        }
        let next = 1u64 << (64 - bits);
        let mask = next - 1;
        let half = next >> 1;
        let low = self.0 & mask;
        let mut r = self.0 & !mask;
        if low > half || (low == half && self.0 & next != 0) {
            r = r.wrapping_add(next);
        }
        Self(r)
    }

    pub(crate) const fn is_negative(self) -> bool {
        self.0 > Self::PI.0
    }

    pub(crate) const fn unsigned_gt(a: Self, b: Self) -> bool {
        a.0 > b.0
    }

    pub(crate) const fn signed_gt(a: Self, b: Self) -> bool {
        a.0.wrapping_add(Angle::PI.0 - 1) > b.0.wrapping_add(Angle::PI.0 - 1)
    }

    pub fn to_radians(self) -> f64 {
        // atan2(0, 1) = pi
        if self == Self::PI {
            return f64::consts::PI;
        }
        (self.0 as i64 as f64) * (f64::consts::TAU / 2.0f64.powi(64))
    }

    #[inline]
    pub(crate) const fn mid(a: Self, b: Self) -> Self {
        let diff = a.0.wrapping_sub(b.0);
        Self(a.0.wrapping_add((diff as i64 >> 1) as u64).wrapping_add(a.0 & diff & 1))
    }

    pub(crate) fn sin_cos(self) -> (f64, f64) {
        let mut d = self;
        let mut inv_sin = false;
        if d.is_negative() {
            inv_sin = true;
            d = -d;
        }
        let mut inv_cos = false;
        if Angle::unsigned_gt(d, Angle::PI_1_2) {
            inv_cos = true;
            d = Angle::PI - d;
        }
        let mut flip = false;
        if Angle::unsigned_gt(d, Angle::PI_1_4) {
            flip = true;
            d = Angle::PI_1_2 - d;
        }
        let (mut x, mut y) = d.to_radians().sin_cos();
        if flip {
            (x, y) = (y, x);
        }
        if inv_cos {
            y = -y;
        }
        if inv_sin {
            x = -x;
        }
        (x, y)
    }

    pub(crate) fn half_sin_cos(self) -> (f64, f64) {
        let mut d = self;
        let mut inv_cos = false;
        if d.is_negative() {
            inv_cos = true;
            d = -d;
        }
        // compute sin(d/2), cos(d/2); then inv_sin, inv_cos
        let mut flip = false;
        if Angle::unsigned_gt(d, Angle::PI_1_2) {
            flip = true;
            d = Angle::PI - d;
        }
        let (mut x, mut y) = (d.to_radians() / 2.0).sin_cos();
        if flip {
            (x, y) = (y, x);
        }
        if inv_cos {
            y = -y;
        }
        (x, y)
    }

    pub(crate) fn half_diff_sin_cos(a: Self, b: Self) -> (f64, f64) {
        let mut inv_sin = false;
        let d = if Angle::signed_gt(a, b) {
            a - b
        } else {
            inv_sin = true;
            b - a
        };
        // compute sin(d/2), cos(d/2); then inv_sin
        let (mut sin, cos) = d.half_sin_cos();
        if inv_sin {
            sin = -sin;
        }
        (sin, cos)
    }

    pub(crate) fn half_sum_sin_cos(a: Self, b: Self) -> (f64, f64) {
        Self::half_diff_sin_cos(a, -b)
    }
}

impl fmt::Display for Angle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if *self == Angle::ZERO {
            return f.write_str("0");
        }
        if *self == Angle::PI {
            return f.write_str("\u{03c0}");
        }
        let mut p: i64 = self.0 as i64;
        let k = p.trailing_zeros();
        let q = 1u64 << (63 - k);
        p >>= k;
        if p == 1 {
            write!(f, "\u{03c0}/{q}")
        } else if p == -1 {
            write!(f, "-\u{03c0}/{q}")
        } else {
            write!(f, "{p}\u{03c0}/{q}")
        }
    }
}

impl fmt::Debug for Angle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        <Self as fmt::Display>::fmt(self, f)
    }
}

impl ops::Neg for Angle {
    type Output = Self;
    #[inline(always)]
    fn neg(self) -> Self {
        Self(self.0.wrapping_neg())
    }
}

impl ops::Add for Angle {
    type Output = Self;
    #[inline(always)]
    fn add(self, other: Self) -> Self {
        Self(self.0.wrapping_add(other.0))
    }
}

impl ops::AddAssign for Angle {
    #[inline(always)]
    fn add_assign(&mut self, other: Self) {
        self.0 = self.0.wrapping_add(other.0);
    }
}

impl ops::Sub for Angle {
    type Output = Self;
    #[inline(always)]
    fn sub(self, other: Self) -> Self {
        Self(self.0.wrapping_sub(other.0))
    }
}

impl ops::SubAssign for Angle {
    #[inline(always)]
    fn sub_assign(&mut self, other: Self) {
        self.0 = self.0.wrapping_sub(other.0);
    }
}

#[cfg(test)]
mod tests {
    use super::Angle;
    use std::f64;

    #[test]
    fn test_const() {
        assert_eq!(Angle::from_radians(0.0), Angle::ZERO);
        assert_eq!(Angle::from_radians(f64::consts::PI).round(40), Angle::PI);
        assert_eq!(Angle::from_radians(-f64::consts::PI).round(40), Angle::PI);
    }

    fn is_small(mut v: f64) -> bool {
        v = v.abs();
        if v.abs() < 1.0e-6 {
            return true;
        }
        if (v - f64::consts::TAU).abs() < 1.0e-6 {
            return true;
        }
        false
    }

    #[test]
    fn test_roundtrip() {
        for a in [0.0, 0.5, 1.0, 2.0, 3.0, 3.1415926536] {
            let aa = Angle::from_radians(a);
            let ba = Angle::from_radians(-a);
            let ar = aa.to_radians();
            let br = ba.to_radians();
            assert!(is_small(a - ar));
            assert!(aa + ba == Angle::ZERO);

            assert!(is_small(ar + br));
        }
    }
}
