// SPDX-License-Identifier: Apache-2.0
use std::{cell::UnsafeCell, marker::PhantomData, ops, ptr, slice};
use std::alloc;

type DropFunc = unsafe fn(at: *mut u8) -> *mut u8;
const _: () = {
    assert!(size_of::<DropFunc>() == size_of::<Option<DropFunc>>());
    assert!(align_of::<DropFunc>() == align_of::<Option<DropFunc>>());
};

struct Chunk {
    ptr: *mut u8,
    alloc_end: *mut u8,
    base: *mut u8,
}

impl Chunk {
    fn new(size: usize) -> Self {
        let ptr = unsafe { alloc::alloc(alloc::Layout::from_size_align(size, align_of::<DropFunc>()).unwrap()) };
        Self {
            ptr,
            alloc_end: unsafe { ptr.add(size) },
            base: ptr,
        }
    }

    #[inline]
    fn try_alloc(&mut self, header: usize, size: usize, align: usize) -> Option<*mut DropFunc> {
        let mut ptr = self.ptr;
        let mut space = unsafe { self.alloc_end.offset_from(ptr) as usize };
        if space < header {
            return None;
        }
        space -= header;
        if space < size {
            return None;
        }
        space -= size;
        let offset = unsafe { ptr.add(header).align_offset(align) };
        if space < offset {
            return None;
        }
        unsafe {
            ptr::write_bytes(ptr, 0, offset);
            ptr = ptr.add(offset);
            self.ptr = ptr.add(header + size);
        }
        Some(ptr as *mut DropFunc)
    }
}

impl Drop for Chunk {
    fn drop(&mut self) {
        let mut at = self.base;
        unsafe {
            loop {
                debug_assert!(at < self.ptr);
                let f = (at as *mut Option<DropFunc>).read();
                at = at.add(size_of::<DropFunc>());
                let Some(f) = f else {
                    continue;
                };
                at = f(at);
                if at == self.ptr {
                    break;
                }
                at = at.add(at.align_offset(align_of::<DropFunc>()));
            }
            let size = self.alloc_end.offset_from(self.base) as usize;
            alloc::dealloc(self.base, alloc::Layout::from_size_align_unchecked(size, align_of::<DropFunc>()));
        }
    }
}

// Arena can't be Send unless all elements are Send -- they are dropped when
// the Arena is dropped.
pub struct Arena {
    mem: UnsafeCell<Vec<Chunk>>,
}

#[repr(transparent)]
pub struct Seq<T> {
    len: usize,
    marker: PhantomData<[T]>,
}

impl<T> ops::Deref for Seq<T> {
    type Target = [T];

    #[inline(always)]
    fn deref(&self) -> &[T] {
        unsafe { slice::from_raw_parts((&raw const *self).add(1) as *const T, self.len) }
    }
}

unsafe fn drop_one<T>(at: *mut u8) -> *mut u8 {
    unsafe {
        ptr::drop_in_place(at as *mut T);
        at.add(size_of::<T>())
    }
}

unsafe fn drop_seq<T>(mut at: *mut u8) -> *mut u8 {
    unsafe {
        let Seq { len, .. } = (at as *const Seq<T>).read();
        at = at.add(size_of::<Seq<T>>());
        ptr::drop_in_place(ptr::slice_from_raw_parts_mut(at as *mut T, len));
        at.add(size_of::<T>() * len)
    }
}

impl Arena {
    pub const fn new() -> Self {
        Self {
            mem: UnsafeCell::new(Vec::new()),
        }
    }

    unsafe fn alloc_bytes(&self, header: usize, size: usize, align: usize) -> *mut DropFunc {
        let mem = unsafe { &mut *self.mem.get() };
        if let Some(chunk) = mem.last_mut() {
            if let Some(ptr) = chunk.try_alloc(header, size, align) {
                return ptr;
            }
        }

        let alloc_size = (size + header + align).max(1024 << (mem.len() / 2));
        mem.push(Chunk::new(alloc_size));

        mem.last_mut().unwrap().try_alloc(header, size, align).unwrap()
    }

    #[allow(clippy::mut_from_ref)]
    pub fn alloc<'a, T>(&'a self, val: T) -> &'a mut T {
        unsafe {
            let base_ptr = self.alloc_bytes(size_of::<DropFunc>(), size_of::<T>(), align_of::<T>().max(align_of::<DropFunc>()));
            base_ptr.write(drop_one::<T>);
            let data_ptr = base_ptr.add(1) as *mut T;
            data_ptr.write(val);
            &mut *data_ptr
        }
    }

    #[allow(clippy::mut_from_ref)]
    pub fn alloc_vec<'a, T>(&'a self, mut val: Vec<T>) -> &'a mut Seq<T> {
        let () = const {
            // Layout assumption.
            assert!(align_of::<DropFunc>() <= align_of::<Seq<T>>());
        };

        unsafe {
            let base_ptr = self.alloc_bytes(size_of::<DropFunc>() + size_of::<Seq<T>>(), size_of::<T>() * val.len(), align_of::<T>().max(align_of::<usize>()));
            base_ptr.write(drop_seq::<T>);
            let seq_ptr = base_ptr.add(1) as *mut Seq<T>;
            seq_ptr.write(Seq {
                len: val.len(),
                marker: PhantomData,
            });
            let data_ptr = seq_ptr.add(1) as *mut T;
            ptr::copy_nonoverlapping(val.as_ptr(), data_ptr, val.len());
            val.set_len(0); // don't drop data, but deallocate
            &mut *seq_ptr
        }
    }
}
