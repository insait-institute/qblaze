// SPDX-License-Identifier: Apache-2.0
use crate::Angle;
use std::{fmt, ops};

/// A complex number.
#[derive(Copy, Clone, PartialEq)]
#[repr(C)]
pub struct Complex(f64, f64);

impl Complex {
    pub(crate) const EPS: f64 = 9.094947017729282e-13f64;
    pub const ZERO: Complex = Complex(0.0, 0.0);
    pub const ONE: Complex = Complex(1.0, 0.0);

    #[inline(always)]
    pub const fn from_cartesian(re: f64, im: f64) -> Self {
        Self(re, im)
    }

    #[inline(always)]
    pub const fn to_cartesian(self) -> (f64, f64) {
        (self.0, self.1)
    }

    #[inline(always)]
    pub const fn re(self) -> f64 {
        self.0
    }

    #[inline(always)]
    pub const fn im(self) -> f64 {
        self.1
    }

    #[inline(always)]
    pub fn conj(self) -> Self {
        Self(self.0, -self.1)
    }

    #[inline(always)]
    pub fn neg_conj(self) -> Self {
        Self(-self.0, self.1)
    }

    #[inline(always)]
    pub fn from_polar(r: f64, theta: Angle) -> Self {
        let (s, c) = theta.sin_cos();
        Self(c * r, s * r)
    }

    #[inline(always)]
    pub fn from_arg(theta: Angle) -> Self {
        let (s, c) = theta.sin_cos();
        Self(c, s)
    }

    #[inline(always)]
    pub fn norm(self) -> f64 {
        self.0.hypot(self.1)
    }

    #[inline(always)]
    pub(crate) fn accumulate(&mut self, a: Complex) -> bool {
        let norm = self.0.abs().max(self.1.abs());
        *self += a;
        self.0.abs().max(self.1.abs()) > Self::EPS * norm
    }

    #[inline(always)]
    pub fn norm2(self) -> f64 {
        self.0 * self.0 + self.1 * self.1
    }

    #[inline(always)]
    pub fn arg(self) -> Angle {
        Angle::from_atan2(self.1, self.0)
    }

    #[inline(always)]
    pub fn to_polar(self) -> (f64, Angle) {
        (self.norm(), self.arg())
    }

    #[inline(always)]
    pub fn mul_i(self) -> Self {
        Self(-self.1, self.0)
    }

    #[inline(always)]
    pub fn div_i(self) -> Self {
        Self(self.1, -self.0)
    }
}

impl From<f64> for Complex {
    #[inline(always)]
    fn from(re: f64) -> Self {
        Self(re, 0.0)
    }
}

impl ops::Neg for Complex {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self(-self.0, -self.1)
    }
}

impl ops::Add for Complex {
    type Output = Self;
    #[inline]
    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0, self.1 + other.1)
    }
}

impl ops::AddAssign for Complex {
    #[inline]
    fn add_assign(&mut self, other: Self) {
        self.0 += other.0;
        self.1 += other.1;
    }
}

impl ops::Add<Complex> for f64 {
    type Output = Complex;
    #[inline]
    fn add(self, other: Complex) -> Complex {
        Complex(self + other.0, other.1)
    }
}

impl ops::Add<f64> for Complex {
    type Output = Self;
    #[inline]
    fn add(self, other: f64) -> Self {
        Self(self.0 + other, self.1)
    }
}

impl ops::AddAssign<f64> for Complex {
    #[inline]
    fn add_assign(&mut self, other: f64) {
        self.0 += other;
    }
}

impl ops::Sub for Complex {
    type Output = Self;
    #[inline]
    fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0, self.1 - other.1)
    }
}

impl ops::SubAssign for Complex {
    #[inline]
    fn sub_assign(&mut self, other: Self) {
        self.0 -= other.0;
        self.1 -= other.1;
    }
}

impl ops::Sub<Complex> for f64 {
    type Output = Complex;
    #[inline]
    fn sub(self, other: Complex) -> Complex {
        Complex(self - other.0, -other.1)
    }
}

impl ops::Sub<f64> for Complex {
    type Output = Self;
    #[inline]
    fn sub(self, other: f64) -> Self {
        Self(self.0 - other, self.1)
    }
}

impl ops::SubAssign<f64> for Complex {
    #[inline]
    fn sub_assign(&mut self, other: f64) {
        self.0 -= other;
    }
}

impl ops::Mul<Complex> for f64 {
    type Output = Complex;
    #[inline]
    fn mul(self, other: Complex) -> Complex {
        Complex(self * other.0, self * other.1)
    }
}

impl ops::Mul<f64> for Complex {
    type Output = Self;
    #[inline]
    fn mul(self, other: f64) -> Self {
        Self(self.0 * other, self.1 * other)
    }
}

impl ops::MulAssign<f64> for Complex {
    #[inline]
    fn mul_assign(&mut self, other: f64) {
        self.0 *= other;
        self.1 *= other;
    }
}

impl ops::Div<f64> for Complex {
    type Output = Self;
    #[inline]
    fn div(self, other: f64) -> Self {
        Self(self.0 / other, self.1 / other)
    }
}

impl ops::DivAssign<f64> for Complex {
    #[inline]
    fn div_assign(&mut self, other: f64) {
        self.0 /= other;
        self.1 /= other;
    }
}

impl ops::Mul for Complex {
    type Output = Self;
    #[inline]
    fn mul(self, other: Self) -> Self {
        Self(self.0 * other.0 - self.1 * other.1, self.0 * other.1 + self.1 * other.0)
    }
}

impl ops::MulAssign for Complex {
    #[inline(always)]
    fn mul_assign(&mut self, other: Complex) {
        *self = *self * other;
    }
}

impl ops::Div for Complex {
    type Output = Self;
    #[inline]
    fn div(self, other: Self) -> Self {
        self * other.conj() / other.norm2()
    }
}

impl ops::DivAssign for Complex {
    #[inline(always)]
    fn div_assign(&mut self, other: Complex) {
        *self = *self / other;
    }
}

impl fmt::Display for Complex {
    fn fmt(&self, w: &mut fmt::Formatter) -> fmt::Result {
        if self.1.abs() <= self.0.abs() * Self::EPS {
            return <f64 as fmt::Display>::fmt(&self.0, w);
        }
        if self.0.abs() > self.1.abs() * Self::EPS {
            <f64 as fmt::Display>::fmt(&self.0, w)?;
            if self.1 > 0.0f64 {
                // FIXME: Once https://github.com/rust-lang/rust/issues/118117 is done, just enable
                // the sign option.
                w.write_str("+")?;
            }
        }
        <f64 as fmt::Display>::fmt(&self.1, w)?;
        w.write_str("j")
    }
}

impl fmt::Debug for Complex {
    fn fmt(&self, w: &mut fmt::Formatter) -> fmt::Result {
        <Self as fmt::Display>::fmt(self, w)
    }
}
