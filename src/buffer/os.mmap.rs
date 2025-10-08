// SPDX-License-Identifier: Apache-2.0
use std::ptr;

const HUGE_PAGE_SIZE: usize = 1 << 21;

#[inline(always)]
fn get_page_size() -> usize {
    (unsafe { libc::sysconf(libc::_SC_PAGESIZE) }) as usize
}

#[inline(always)]
pub unsafe fn dealloc(p: *mut u8, bytes: usize, _align: usize) {
    unsafe { libc::munmap(p as *mut _, bytes); }
}

pub unsafe fn realloc(old_p: *mut u8, old_bytes: usize, mut bytes: usize, mut align: usize) -> (*mut u8, usize) {
    let page_size = get_page_size();
    align = align.max(HUGE_PAGE_SIZE);
    let mut flags = 0;
    if old_bytes == 0 {
        flags = libc::PROT_READ | libc::PROT_WRITE;
    }
    bytes += bytes.wrapping_neg() & (align - 1);
    bytes += align - page_size;

    let mut p = unsafe { libc::mmap(
        ptr::null_mut(),
        bytes,
        flags,
        libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
        -1,
        0,
    ) };
    if p == libc::MAP_FAILED {
        return (ptr::null_mut(), 0);
    }

    let align_left = (p as usize).wrapping_neg() & (align - 1);
    if align_left != 0 {
        unsafe {
            libc::munmap(p, align_left);
            p = p.byte_add(align_left);
        }
        bytes -= align_left;
    }
    let align_right = bytes & (align - 1);
    if align_right != 0 {
        bytes -= align_right;
        unsafe {
            libc::munmap(p.byte_add(bytes), align_right);
        }
    }

    if old_bytes != 0 {
        unsafe {
            let r = libc::mremap(old_p as *mut _, old_bytes, bytes, libc::MREMAP_FIXED | libc::MREMAP_MAYMOVE, p);
            if r != p {
                libc::abort();
            }
        }
    }

    unsafe {
        let _ = libc::madvise(p as *mut _, bytes, libc::MADV_HUGEPAGE);
    }

    (p as *mut _, bytes)
}

#[cfg(feature = "mem_release")]
pub unsafe fn trim(ptr: *mut u8, cap: usize, mut offset: usize) {
    offset += offset.wrapping_neg() & (get_page_size() - 1);
    let rest = cap - offset;
    if rest == 0 {
        return;
    }
    unsafe {
        libc::madvise(ptr.byte_add(offset) as *mut _, rest, libc::MADV_FREE);
    }
}

#[cfg(not(feature = "mem_release"))]
#[inline(always)]
pub unsafe fn trim(_ptr: *mut u8, _cap: usize, _offset: usize) {}
