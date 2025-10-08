// SPDX-License-Identifier: Apache-2.0
//! Mostly based on the Rust standard library, `library/core/src/slice/sort.rs`

/*
Short version for non-lawyers:

The Rust Project is dual-licensed under Apache 2.0 and MIT
terms.


Longer version:

Copyrights in the Rust project are retained by their contributors. No
copyright assignment is required to contribute to the Rust project.

Some files include explicit copyright notices and/or license notices.
For full authorship information, see the version control history or
https://thanks.rust-lang.org

Except as otherwise noted (below and/or in individual files), Rust is
licensed under the Apache License, Version 2.0 <LICENSE-APACHE> or
<http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
<LICENSE-MIT> or <http://opensource.org/licenses/MIT>, at your option.
*/

use std::{mem::{self, MaybeUninit}, ptr};
use crate::threading;


/// Sorts range [v_base, tail] assuming [v_base, tail) is already sorted.
///
/// SAFETY: [v_base, tail] must be valid and initialized. tail > v_base.
#[inline(always)]
unsafe fn insert_tail<T, F>(v_base: *mut T, tail: *mut T, is_less: F)
where
    T: Copy,
    F: Copy + Fn(&T, &T) -> bool,
{
    unsafe {
        let mut sift = tail.sub(1);
        if !is_less(&*tail, &*sift) {
            return;
        }

        let tmp = tail.read();
        let mut gap;
        loop {
            ptr::copy_nonoverlapping(sift, sift.add(1), 1);
            gap = sift;
            if sift == v_base {
                break;
            }
            sift = sift.sub(1);
            if !is_less(&tmp, &*sift) {
                break;
            }
        }
        gap.write(tmp);
    }
}

/// Sort `v`
#[inline(always)]
fn insertion_sort<T, F>(v: &mut [T], is_less: F)
where
    T: Copy,
    F: Copy + Fn(&T, &T) -> bool,
{
    for i in 1..v.len() {
        // SAFETY: 1 <= i < v.len()
        unsafe {
            insert_tail(v.as_mut_ptr(), v.as_mut_ptr().add(i), is_less);
        }
    }
}

pub fn partition_by<T, F>(v: &mut [T], is_less: F) -> usize
where
    T: Copy,
    F: Copy + Fn(&T) -> bool,
{
    let len = v.len();
    if len == 0 {
        return 0;
    }

    let mut gap_value = MaybeUninit::<T>::uninit();
    let mut gap_pos = gap_value.as_mut_ptr();
    unsafe {
        let v_base = v.as_mut_ptr();

        let mut left = v_base;
        let mut right = v_base.add(len);

        'outer: loop {
            while left < right && is_less(&*left) {
                left = left.add(1);
            }
            loop {
                right = right.sub(1);
                if left >= right {
                    break 'outer;
                }
                if is_less(&*right) {
                    break;
                }
            }

            debug_assert!(left < right);
            debug_assert!(gap_pos == gap_value.as_mut_ptr() || right < gap_pos);
            ptr::copy_nonoverlapping(left, gap_pos, 1);
            ptr::copy_nonoverlapping(right, left, 1);
            gap_pos = right;
            left = left.add(1);
        }

        // may overlap
        ptr::copy(gap_value.as_ptr(), gap_pos, 1);
        left.offset_from(v_base) as usize
    }
}

/// Chooses a pivot in `v` and returns the index and `true` if the slice is likely already sorted.
///
/// Elements in `v` might be reordered in the process.
fn choose_pivot<T, F>(v: &mut [T], is_less: F) -> usize
where
    F: Copy + Fn(&T, &T) -> bool,
{
    // Minimum length to choose the median-of-medians-of-three method.
    // Shorter slices use the simple median-of-three method.
    const SHORTEST_MEDIAN_OF_MEDIANS: usize = 100;

    let len = v.len();

    // SAFETY: `len` is large enough to ensure that everything is in bounds and that there is no
    // aliasing.
    unsafe {
        // Three indices near which we are going to choose a pivot.
        let mut a = v.as_mut_ptr().add(len / 4 * 1);
        let mut b = v.as_mut_ptr().add(len / 4 * 2);
        let mut c = v.as_mut_ptr().add(len / 4 * 3);

        if len >= 8 {
            // Swaps indices so that `v[a] <= v[b]`.
            // SAFETY: `len >= 8` so there are at least two elements in the neighborhoods of
            // `a`, `b` and `c`. This means the three calls to `sort_adjacent` result in
            // corresponding calls to `sort3` with valid 3-item neighborhoods around each
            // pointer, which in turn means the calls to `sort2` are done with valid
            // references. Thus the `v.get_unchecked` calls are safe, as is the `ptr::swap`
            // call.
            let sort2 = |a: &mut *mut T, b: &mut *mut T| {
                if is_less(&**a, &**b) {
                    ptr::swap(a, b);
                }
            };

            // Swaps indices so that `v[a] <= v[b] <= v[c]`.
            let sort3 = |a: &mut *mut T, b: &mut *mut T, c: &mut *mut T| {
                sort2(a, b);
                sort2(b, c);
                sort2(a, b);
            };

            if len >= SHORTEST_MEDIAN_OF_MEDIANS {
                // Finds the median of `*a.sub(1), *a, *a.add(1)` and store the index into `a`.
                let sort_adjacent = |a: &mut *mut T| {
                    let tmp = *a;
                    sort3(&mut tmp.sub(1), a, &mut tmp.add(1));
                };

                // Find medians in the neighborhoods of `a`, `b`, and `c`.
                sort_adjacent(&mut a);
                sort_adjacent(&mut b);
                sort_adjacent(&mut c);
            }

            // Find the median among `a`, `b`, and `c`.
            sort3(&mut a, &mut b, &mut c);
        }

        b.offset_from(v.as_ptr()) as usize
    }
}

#[inline(never)]
fn partition_pivot_impl<T, F>(v: &mut [T], is_less: F) -> usize
where
    T: Copy,
    F: Copy + Fn(&T, &T) -> bool,
{
    let pivot = choose_pivot(v, is_less);
    v.swap(0, pivot);
    let (pivot, v_without_pivot) = v.split_at_mut(1);
    let pivot = &pivot[0];

    let num_lt = partition_by(v_without_pivot, #[inline(always)] move |v: &T| is_less(v, pivot));

    v.swap(0, num_lt);
    num_lt
}

#[inline(always)]
fn partition_pivot<T, F>(v: &mut [T], is_less: F) -> (&mut [T], &mut T, &mut [T])
where
    T: Copy,
    F: Copy + Fn(&T, &T) -> bool,
{
    let num_lt = partition_pivot_impl(v, is_less);
    unsafe {
        let (left, right) = v.split_at_mut_unchecked(num_lt);
        let (pivot, right) = right.split_first_mut().unwrap_unchecked();
        (left, pivot, right)
    }
}

#[inline(never)]
pub fn seq_quicksort<'a, T, F>(
    mut v: &'a mut [T],
    is_less: F,
) where
    T: Copy,
    F: Copy + Fn(&T, &T) -> bool,
{
    // Sorting has no meaningful behavior on zero-sized types.
    if const { mem::size_of::<T>() == 0 } {
        return;
    }

    let max_insertion: usize = 20;

    while v.len() > max_insertion {
        let (left, _, right) = partition_pivot(v, is_less);
        seq_quicksort(left, is_less);
        v = right;
    }

    insertion_sort::<T, F>(v, is_less);
}

#[inline(never)]
fn par_recurse<'a, T, F>(
    sc: threading::Scope<'a>,
    mut v: &'a mut [T],
    is_less: F,
) where
    T: Send + Copy,
    F: 'a + Send + Copy + Fn(&T, &T) -> bool,
{
    let max_seq: usize = const { (1 << 16) / mem::size_of::<T>() }.max(40);

    while v.len() > max_seq {
        let (left, _, right) = partition_pivot(v, is_less);

        // Submit the smaller side to the thread pool and continue with the larger side.
        let submit = if left.len() <= right.len() {
            v = right;
            left
        } else {
            v = left;
            right
        };
        if submit.len() > 1 {
            sc.submit_one(move |sc| par_recurse(sc, submit, is_less));
        }
    }

    seq_quicksort(v, is_less);
}

#[inline]
pub fn par_quicksort<'a, T, F>(sc: threading::Scope<'a>, v: &'a mut [T], is_less: F)
where
    T: Send + Copy,
    F: 'a + Send + Copy + Fn(&T, &T) -> bool,
{
    // Sorting has no meaningful behavior on zero-sized types.
    if const { mem::size_of::<T>() == 0 } {
        return;
    }

    if sc.is_single_threaded() {
        seq_quicksort(v, is_less);
        return;
    }
    par_recurse(sc, v, is_less);
}
