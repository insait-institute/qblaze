// SPDX-License-Identifier: Apache-2.0
use std::alloc;

#[inline(always)]
pub unsafe fn dealloc(p: *mut u8, bytes: usize, align: usize) {
    unsafe { alloc::dealloc(p, alloc::Layout::from_size_align_unchecked(bytes, align)) };
}

#[inline(always)]
pub unsafe fn realloc(p: *mut u8, old_bytes: usize, bytes: usize, align: usize) -> (*mut u8, usize) {
    let p = if old_bytes == 0 {
        unsafe { alloc::alloc(alloc::Layout::from_size_align_unchecked(bytes, align)) }
    } else {
        unsafe { alloc::realloc(p, alloc::Layout::from_size_align_unchecked(old_bytes, align), bytes) }
    };
    (p, bytes)
}

pub unsafe fn trim(_ptr: *mut u8, _cap: usize, _offset: usize) {}
