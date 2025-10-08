// SPDX-License-Identifier: Apache-2.0
use std::{ptr, sync::atomic::AtomicU32};

#[inline]
pub fn wait(fut: &AtomicU32, v: u32, mask: u32) {
    unsafe {
        libc::syscall(
            libc::SYS_futex,
            fut as *const AtomicU32,
            libc::FUTEX_WAIT_BITSET | libc::FUTEX_PRIVATE_FLAG,
            v,
            ptr::null::<u8>(),
            ptr::null::<u8>(),
            mask,
        );
    }
}

#[inline]
pub fn wake_one(fut: &AtomicU32, mask: u32) -> bool {
    unsafe {
        libc::syscall(
            libc::SYS_futex,
            fut as *const AtomicU32,
            libc::FUTEX_WAKE_BITSET | libc::FUTEX_PRIVATE_FLAG,
            1i32,
            ptr::null::<u8>(),
            ptr::null::<u8>(),
            mask,
        ) > 0
    }
}

#[inline]
pub fn wake_all(fut: &AtomicU32, mask: u32) -> bool {
    unsafe {
        libc::syscall(
            libc::SYS_futex,
            fut as *const AtomicU32,
            libc::FUTEX_WAKE_BITSET | libc::FUTEX_PRIVATE_FLAG,
            i32::MAX,
            ptr::null::<u8>(),
            ptr::null::<u8>(),
            mask,
        ) > 0
    }
}
