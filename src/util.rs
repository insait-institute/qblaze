// SPDX-License-Identifier: Apache-2.0
use std::cmp;

fn merge_boundaries<T, K: Ord>(c0: &[T], c1: &[T], target: usize, f: impl Fn(&T) -> K) -> (usize, usize) {
    let mut n0_min = target.saturating_sub(c1.len());
    let mut n0_max = c0.len().min(target);
    loop {
        let n0 = (n0_min + n0_max) >> 1;
        let n1 = target - n0;
        if n0 > 0 && n1 < c1.len() {
            match f(&c0[n0 - 1]).cmp(&f(&c1[n1])) {
                cmp::Ordering::Equal => {
                    // Matching keys, so don't take the last item.
                    return (n0 - 1, n1);
                }
                cmp::Ordering::Greater => {
                    // We've taken too much from c0.
                    n0_max = n0 - 1;
                    assert!(n0_min <= n0_max);
                    continue;
                }
                cmp::Ordering::Less => {}
            }
        }
        if n1 > 0 && n0 < c0.len() {
            match f(&c0[n0]).cmp(&f(&c1[n1 - 1])) {
                cmp::Ordering::Equal => {
                    // Matching keys, so don't take the last item.
                    return (n0, n1 - 1);
                }
                cmp::Ordering::Less => {
                    // We've taken too much from c1.
                    n0_min = n0 + 1;
                    assert!(n0_min <= n0_max);
                    continue;
                }
                cmp::Ordering::Greater => {}
            }
        }
        // From the comparisons:
        //   c0[n0 - 1] < c1[n1]
        //   c1[n1 - 1] < c0[n0]
        // From invariants:
        //   c0[n0 - 1] < c0[n0]
        //   c1[n1 - 1] < c1[n1]
        // Therefore we are done.
        return (n0, n1);
    }
}

pub fn merge_chunk_pairs<'a, 'b, T, K: Ord>(c0: &'a [T], c1: &'b [T], count: usize, f: impl Fn(&T) -> K) -> Vec<(&'a [T], &'b [T])> {
    debug_assert!(count > 0);
    let mut r = Vec::with_capacity(count);
    let n = c0.len() + c1.len();
    let mut at0 = 0;
    let mut at1 = 0;
    for i in 1..count {
        let target = (n * i / count).saturating_sub(at0 + at1);
        if target == 0 {
            continue;
        }
        let (n0, n1) = merge_boundaries(&c0[at0..], &c1[at1..], target, &f);
        let end0 = at0 + n0;
        let end1 = at1 + n1;
        r.push((&c0[at0..end0], &c1[at1..end1]));
        at0 = end0;
        at1 = end1;
    }
    if at0 < c0.len() || at1 < c1.len() {
        r.push((&c0[at0..], &c1[at1..]));
    }
    r
}

pub fn merge_distinct<T: Copy, K: Ord>(
    mut v1: &[T],
    mut v2: &[T],
    f: impl Fn(&T) -> K,
    mut w: impl FnMut(T),
) {
    while !v1.is_empty() && !v2.is_empty() {
        let (h1, t1) = v1.split_first().unwrap();
        let (h2, t2) = v2.split_first().unwrap();
        if f(h1) <= f(h2) {
            w(*h1);
            v1 = t1;
        } else {
            w(*h2);
            v2 = t2;
        }
    }
    for item in v1.iter() {
        w(*item);
    }
    for item in v2.iter() {
        w(*item);
    }
}

pub enum MergeEq<T> {
    Lhs(T),
    Rhs(T),
    Both(T, T),
}

pub fn merge_eq<T, K: Ord, FK: Fn(&T) -> K, FW: FnMut(MergeEq<T>)>(
    v1: impl IntoIterator<Item = T>,
    v2: impl IntoIterator<Item = T>,
    f: FK,
    mut w: FW,
) {
    let mut v1 = v1.into_iter();
    let mut v2 = v2.into_iter();
    let mut h1 = v1.next();
    let mut h2 = v2.next();
    while h1.is_some() && h2.is_some() {
        let e1 = unsafe { h1.unwrap_unchecked() };
        let e2 = unsafe { h2.unwrap_unchecked() };
        match f(&e1).cmp(&f(&e2)) {
            cmp::Ordering::Equal => {
                w(MergeEq::Both(e1, e2));
                h1 = v1.next();
                h2 = v2.next();
            }
            cmp::Ordering::Less => {
                w(MergeEq::Lhs(e1));
                h1 = v1.next();
                h2 = Some(e2);
            }
            cmp::Ordering::Greater => {
                w(MergeEq::Rhs(e2));
                h1 = Some(e1);
                h2 = v2.next();
            }
        }
    }
    while let Some(e1) = h1 {
        w(MergeEq::Lhs(e1));
        h1 = v1.next();
    }
    while let Some(e2) = h2 {
        w(MergeEq::Rhs(e2));
        h2 = v2.next();
    }
}

#[cfg(test)]
mod test {
    use super::merge_boundaries;

    fn unpack_bits(a: &mut [u32], mut mask: u32) -> usize {
        let mut i = 0;
        while mask != 0 {
            a[i] = mask.trailing_zeros();
            i += 1;
            mask &= mask - 1;
        }
        i
    }

    #[test]
    fn test_merge_threshold() {
        const N: usize = 12;
        let mut c0 = [0u32; N];
        let mut c1 = [0u32; N];

        // Just test all possibilities:
        // - each array is a sorted subset of {0..N-1}
        // - try getting all boundaries.
        for s0 in 0..(1 << N) {
            for s1 in 0..(1 << N) {
                let l0 = unpack_bits(&mut c0[..], s0);
                let l1 = unpack_bits(&mut c1[..], s1);
                for i in 0..=(l0 + l1) {
                    let (n0, n1) = merge_boundaries(&c0[..l0], &c1[..l1], i, |a| *a);
                    assert!(n0 + n1 == i || n0 + n1 + 1 == i);
                    assert!(n0 == 0 || n1 == l1 || c0[n0 - 1] < c1[n1]);
                    assert!(n1 == 0 || n0 == l0 || c1[n1 - 1] < c0[n0]);
                }
            }
        }
    }
}
