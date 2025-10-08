// SPDX-License-Identifier: Apache-2.0
use crate::Angle;
use std::fmt;

/// Single-qubit gate up to global phase.
#[derive(Copy, Clone, PartialEq)]
pub struct U3 {
    // rz(lambda); ry(theta); rz(phi)
    // theta in [0; pi]
    // phi in [0; 2pi)
    // lambda in [0; 2pi)
    pub theta: Angle,
    pub phi: Angle,
    pub lambda: Angle,
}

impl U3 {
    /// The identity gate.
    pub const I: Self = Self { theta: Angle::ZERO, phi: Angle::ZERO, lambda: Angle::ZERO };
    /// The Pauli-X gate.
    pub const X: Self = Self { theta: Angle::PI, phi: Angle::PI, lambda: Angle::ZERO };
    /// The Pauli-Y gate.
    pub const Z: Self = Self { theta: Angle::ZERO, phi: Angle::PI, lambda: Angle::ZERO };
    /// The Pauli-Z gate.
    pub const Y: Self = Self { theta: Angle::PI, phi: Angle::ZERO, lambda: Angle::ZERO };
    /// The Hadamard gate.
    pub const H: Self = Self { theta: Angle::PI_1_2, phi: Angle::ZERO, lambda: Angle::PI };

    /// Create a new U3 gate with the given angles.
    pub fn new(mut theta: Angle, mut phi: Angle, mut lambda: Angle) -> Self {
        if theta == Angle::ZERO {
            phi += lambda;
            lambda = Angle::ZERO;
        } else if theta == Angle::PI {
            phi -= lambda;
            lambda = Angle::ZERO;
        } else if Angle::is_negative(theta) {
            theta = -theta;
            phi += Angle::PI;
            lambda += Angle::PI;
        }
        Self { theta, phi, lambda }
    }

    /// Create a U3 gate corresponding to rotation around the X axis.
    #[inline]
    pub fn rx(a: Angle) -> Self {
        Self::new(a, Angle::PI_3_2, Angle::PI_1_2)
    }

    /// Create a U3 gate corresponding to rotation around the Y axis.
    #[inline]
    pub fn ry(a: Angle) -> Self {
        Self::new(a, Angle::ZERO, Angle::ZERO)
    }

    /// Create a U3 gate corresponding to rotation around the Z axis.
    #[inline]
    pub fn rz(a: Angle) -> Self {
        Self {
            theta: Angle::ZERO,
            phi: a,
            lambda: Angle::ZERO,
        }
    }

    fn from_yzy(ry1: Angle, rz: Angle, ry2: Angle) -> Self {
        if rz == Angle::ZERO {
            return Self::ry(ry1 + ry2);
        }
        if rz == Angle::PI {
            return Self::new(ry2 - ry1, Angle::ZERO, Angle::PI);
        }
        if ry1 == Angle::ZERO {
            return Self::new(ry2, Angle::ZERO, rz);
        }
        if ry1 == Angle::PI {
            return Self::new(ry2 + Angle::PI, Angle::ZERO, -rz);
        }
        if ry2 == Angle::ZERO {
            return Self::new(ry1, rz, Angle::ZERO);
        }
        if ry2 == Angle::PI {
            return Self::new(ry1 + Angle::PI, -rz, Angle::ZERO);
        }
        if ry1 == Angle::PI_1_2 && ry2 == Angle::PI_1_2 {
            return Self::new(Angle::PI - rz, Angle::PI_1_2, Angle::PI_1_2);
        }

        let (shd, chd) = Angle::half_diff_sin_cos(ry1, ry2);
        let (shs, chs) = Angle::half_sum_sin_cos(ry1, ry2);
        let m10_re;
        let m10_im;
        let m11_re;
        let m11_im;

        // Don't multiply by sqrt(1/2) if not necessary.
        if rz == Angle::PI_1_2 {
            m10_re = shs;
            m10_im = shd;
            m11_re = chs;
            m11_im = chd;
        } else if rz == Angle::PI_3_2 {
            m10_re = -shs;
            m10_im = shd;
            m11_re = -chs;
            m11_im = chd;
        } else {
            let (shz, chz) = rz.half_sin_cos();
            m10_re = chz * shs;
            m10_im = shz * shd;
            m11_re = chz * chs;
            m11_im = shz * chd;
        }
        let theta = Angle::from_2atan2_pos(f64::hypot(m10_re, m10_im), f64::hypot(m11_re, m11_im));
        let half_phi_minus_lambda = Angle::from_atan2(m10_im, m10_re);
        let half_phi_plus_lambda = Angle::from_atan2(m11_im, m11_re);
        let phi = half_phi_plus_lambda + half_phi_minus_lambda;
        let lambda = half_phi_plus_lambda - half_phi_minus_lambda;
        Self::new(theta, phi, lambda)
    }

    /// Compose a pair of U3 gates. `a` is applied before `b`.
    pub fn compose(a: Self, b: Self) -> Self {
        // rz(a.lambda)
        // ry(a.theta)
        // rz(a.phi + b.lambda)
        // ry(b.theta)
        // rz(b.phi)
        let r = Self::from_yzy(a.theta, a.phi + b.lambda, b.theta);
        Self::new(r.theta, r.phi + b.phi, a.lambda + r.lambda)
    }

    /// Compute the inverse of a U3 gate.
    pub fn adjoint(self) -> Self {
        Self::new(-self.theta, -self.lambda, -self.phi)
    }

    pub(crate) fn is_sfree(self, prec: u8) -> bool {
        let th = self.theta.round(prec);
        th == Angle::ZERO || th == Angle::PI
    }

    pub(crate) fn to_cx_rz(self, prec: u8) -> Option<(bool, Angle)> {
        match self.theta.round(prec) {
            Angle::ZERO => Some((false, self.phi + self.lambda)),
            Angle::PI => Some((true, Angle::PI + self.phi - self.lambda)),
            _ => None,
        }
    }

    pub(crate) fn to_rz_rx(self, prec: u8) -> Option<(Angle, Angle)> {
        let th = self.theta.round(prec);
        if th == Angle::ZERO {
            // rz
            return Some((self.phi + self.lambda, Angle::ZERO));
        }
        if th == Angle::PI {
            // rz; x
            return Some((Angle::PI - self.phi + self.lambda, Angle::PI));
        }
        let ph = self.phi.round(prec);
        if ph == Angle::PI_3_2 {
            // rz(lambda-pi/2); rx(theta) = { rz(pi/2); ry(theta); rz(3pi/2) }
            return Some((self.lambda - Angle::PI_1_2, self.theta));
        }
        if ph == Angle::PI_1_2 {
            // rz(lambda+pi/2); rx(-theta) = { rz(pi/2); z; ry(theta); z; rz(3pi/2) }
            return Some((self.lambda + Angle::PI_1_2, -self.theta));
        }
        None
    }

    pub(crate) fn to_rz_h_rz(self, prec: u8) -> Option<(Angle, Angle)> {
        // rz(lambda); ry(theta); rz(phi)
        if self.theta.round(prec) != Angle::PI_1_2 {
            return None;
        }
        Some((self.lambda - Angle::PI, self.phi))
    }
}

impl fmt::Debug for U3 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fn write_rz(phi: Angle, f: &mut fmt::Formatter) -> fmt::Result {
            match phi {
                Angle::PI_1_4 => f.write_str("t"),
                Angle::PI_1_2 => f.write_str("s"),
                Angle::PI => f.write_str("z"),
                Angle::PI_3_2 => f.write_str("sdg"),
                Angle::PI_7_4 => f.write_str("tdg"),
                _ => write!(f, "rz({phi})"),
            }
        }

        match *self {
            U3::I => f.write_str("I"),
            U3::X => f.write_str("x"),
            U3::Y => f.write_str("y"),
            U3::Z => f.write_str("z"),
            U3::H => f.write_str("h"),
            U3 { theta: Angle::ZERO, phi, lambda } => {
                debug_assert!(lambda == Angle::ZERO);
                write_rz(phi, f)
            }
            U3 { theta, phi: Angle::ZERO, lambda: Angle::ZERO } => write!(f, "ry({theta})"),
            U3 { theta, phi: Angle::PI, lambda: Angle::PI } => write!(f, "ry({})", -theta),
            U3 { theta, phi: Angle::PI_3_2, lambda: Angle::PI_1_2 } => write!(f, "rx({theta})"),
            U3 { theta, phi: Angle::PI_1_2, lambda: Angle::PI_3_2 } => write!(f, "rx({})", -theta),
            U3 { theta: Angle::PI, phi, lambda } => {
                debug_assert!(lambda == Angle::ZERO);
                f.write_str("x; ")?;
                write_rz(phi + Angle::PI, f)
            },
            U3 { theta: Angle::PI_1_2, phi, lambda } => {
                if lambda != Angle::PI {
                    write_rz(lambda + Angle::PI, f)?;
                    f.write_str("; ")?;
                }
                f.write_str("h")?;
                if phi != Angle::ZERO {
                    f.write_str("; ")?;
                    write_rz(phi, f)?;
                }
                Ok(())
            },
            _ => write!(f, "u3({}, {}, {})", self.theta, self.phi, self.lambda)
        }
    }
}

impl Default for U3 {
    #[inline(always)]
    fn default() -> Self {
        Self::I
    }
}

#[cfg(test)]
mod tests {
    use crate::{Angle, U3};

    fn half_angles(n: u32) -> impl Iterator<Item = Angle> {
        let m = n / 2;
        (0..=m).map(move |i| Angle::from_turns(i as f64 / n as f64))
    }

    fn angles(n: u32) -> impl Iterator<Item = Angle> {
        (0..n).map(move |i| Angle::from_turns(i as f64 / n as f64))
    }

    fn u2s(n: u32) -> impl Iterator<Item = U3> {
        half_angles(n).flat_map(move |theta| angles(n).map(move |phi| {
            U3::new(theta, phi, Angle::ZERO)
        }))
    }

    fn u3s(n: u32) -> impl Iterator<Item = U3> {
        half_angles(n).flat_map(move |theta| angles(n).flat_map(move |phi| angles(n).map(move |lambda| {
            U3::new(theta, phi, lambda)
        })))
    }

    fn comp(s: impl IntoIterator<Item = U3>) -> U3 {
        s.into_iter().reduce(U3::compose).unwrap_or(U3::I)
    }

    #[test]
    fn consts() {
        assert_eq!(U3::rx(Angle::ZERO), U3::I);
        assert_eq!(U3::rx(Angle::PI), U3::X);
        assert_eq!(U3::ry(Angle::ZERO), U3::I);
        assert_eq!(U3::ry(Angle::PI), U3::Y);
        assert_eq!(U3::rz(Angle::ZERO), U3::I);
        assert_eq!(U3::rz(Angle::PI), U3::Z);
    }

    #[test]
    fn pauli_identities() {
        assert_eq!(comp([U3::X, U3::X]), U3::I);
        assert_eq!(comp([U3::Y, U3::Y]), U3::I);
        assert_eq!(comp([U3::Z, U3::Z]), U3::I);
        assert_eq!(comp([U3::Z, U3::X]), U3::Y);
        assert_eq!(comp([U3::X, U3::Z]), U3::Y);
        assert_eq!(comp([U3::X, U3::Y]), U3::Z);
        assert_eq!(comp([U3::Y, U3::X]), U3::Z);
        assert_eq!(comp([U3::Y, U3::Z]), U3::X);
        assert_eq!(comp([U3::Z, U3::Y]), U3::X);
    }

    #[test]
    fn ryz_composition() {
        for a in angles(256) {
            for b in angles(256) {
                assert_eq!(comp([U3::rz(a), U3::rz(b)]), U3::rz(a + b));
                assert_eq!(comp([U3::ry(a), U3::ry(b)]), U3::ry(a + b));
            }
        }
    }

    #[test]
    fn rx_composition() {
        for a in angles(256) {
            for b in angles(256) {
                assert_eq!(comp([U3::rx(a), U3::rx(b)]), U3::rx(a + b));
            }
        }
    }

    #[test]
    fn swap_x_rz() {
        for a in angles(256) {
            assert_eq!(comp([U3::X, U3::rz(a)]), comp([U3::rz(-a), U3::X]));
        }
    }

    #[test]
    fn swap_z_rx() {
        for a in angles(256) {
            assert_eq!(comp([U3::Z, U3::rx(a)]), comp([U3::rx(-a), U3::Z]));
        }
    }

    #[test]
    fn swap_h_rz() {
        for a in angles(256) {
            assert_eq!(comp([U3::H, U3::rz(a), U3::H]), U3::rx(a));
            assert_eq!(comp([U3::H, U3::rz(a)]), comp([U3::rx(a), U3::H]));
        }
    }

    #[test]
    fn swap_h_rx() {
        for a in angles(256) {
            assert_eq!(comp([U3::H, U3::rx(a), U3::H]), U3::rz(a));
            assert_eq!(comp([U3::H, U3::rx(a)]), comp([U3::rz(a), U3::H]));
        }
    }

    fn round(a: U3) -> U3 {
        U3::new(a.theta.round(40), a.phi.round(40), a.lambda.round(40))
    }

    #[test]
    fn adjoint() {
        for r1 in u2s(16) {
            for r2 in u2s(16) {
                let lhs = round(U3::compose(r1, r2).adjoint());
                let rhs = round(U3::compose(r2.adjoint(), r1.adjoint()));
                assert_eq!(lhs, rhs);
            }
        }
    }

    #[test]
    fn adjoint2() {
        for r1 in u2s(32) {
            for r2 in u2s(32) {
                assert_eq!(round(U3::compose(U3::compose(r1, r2.adjoint()), r2)), r1);
            }
        }
    }

    #[test]
    fn assoc() {
        for r1 in u2s(16) {
            for r2 in u2s(16) {
                for r3 in u2s(16) {
                    let lhs = U3::compose(U3::compose(r1, r2), r3);
                    let rhs = U3::compose(r1, U3::compose(r2, r3));
                    assert_eq!(round(lhs), round(rhs));
                }
            }
        }
    }

    #[test]
    fn to_rz_rx() {
        for r1 in u3s(64) {
            if let Some((rz, rx)) = r1.to_rz_rx(40) {
                let r2 = U3::compose(U3::rz(rz), U3::rx(rx));
                assert_eq!(round(r1), round(r2));
            }
        }
        for rz in angles(256) {
            for rx in angles(256) {
                assert_eq!(U3::compose(U3::rz(rz), U3::rx(rx)).to_rz_rx(40), Some((rz, rx)));
            }
        }
    }

    #[test]
    fn to_cx_rz() {
        for r1 in u3s(64) {
            if let Some((is_x, rz)) = r1.to_cx_rz(40) {
                let r2 = U3::compose(if is_x { U3::X } else { U3::I }, U3::rz(rz));
                assert_eq!(round(r1), round(r2));
            }
        }
        for rz in angles(256) {
            assert_eq!(U3::rz(rz).to_cx_rz(40), Some((false, rz)));
            assert_eq!(U3::compose(U3::X, U3::rz(rz)).to_cx_rz(40), Some((true, rz)));
        }
    }

    #[test]
    fn to_rz_h_rz() {
        for r1 in u3s(64) {
            if let Some((rz1, rz2)) = r1.to_rz_h_rz(40) {
                let r2 = U3::compose(U3::rz(rz1), U3::compose(U3::H, U3::rz(rz2)));
                assert_eq!(round(r1), round(r2));
            }
        }
        for rz1 in angles(256) {
            for rz2 in angles(256) {
                assert_eq!(U3::compose(U3::rz(rz1), U3::compose(U3::H, U3::rz(rz2))).to_rz_h_rz(40), Some((rz1, rz2)));
            }
        }
    }
}
