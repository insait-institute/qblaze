// SPDX-License-Identifier: Apache-2.0
use crate::{Complex, U3};

// Column-major with the second column reversed, i.e.
//                              / v00    v11 \
// [[v00, v01], [v10, v11]] == |              |
//                              \ v01    v10 /
#[derive(Copy, Clone)]
pub struct U4([[Complex; 2]; 2]);

impl From<U3> for U4 {
    fn from(r: U3) -> Self {
        let (sin_half_theta, cos_half_theta) = r.theta.half_sin_cos();
        let v_phi = Complex::from_arg(r.phi);
        let v_lambda = Complex::from_arg(r.lambda);
        let v_sum = Complex::from_arg(r.phi + r.lambda);
        Self([
            [
                Complex::from(cos_half_theta),
                v_phi * sin_half_theta,
            ],
            [
                v_sum * cos_half_theta,
                v_lambda * -sin_half_theta,
            ],
        ])
    }
}

impl U4 {
    #[inline(always)]
    pub fn apply(&self, v: bool, a: Complex) -> (Complex, Complex) {
        let d = self.0[v as usize];
        (d[0] * a, d[1] * a)
    }
}
