// SPDX-License-Identifier: Apache-2.0
use std::{array, marker::PhantomData, mem, ops, ptr, slice};

pub struct MemoryError;

#[cfg_attr(all(target_os = "linux", feature = "mem_mmap"), path = "buffer/os.mmap.rs")]
mod os;

pub struct RawBuf<T> {
    ptr: ptr::NonNull<T>,
    capacity_bytes: usize,
}

impl<T> Default for RawBuf<T> {
    #[inline(always)]
    fn default() -> Self {
        Self::empty()
    }
}

unsafe impl<T> Send for RawBuf<T> {}
unsafe impl<T> Sync for RawBuf<T> {}

impl<T> Drop for RawBuf<T> {
    #[inline(always)]
    fn drop(&mut self) {
        if self.capacity_bytes == 0 {
            return;
        }
        unsafe {
            os::dealloc(
                self.ptr.as_ptr() as *mut u8,
                self.capacity_bytes,
                mem::align_of::<T>(),
            );
        }
    }
}

impl<T> RawBuf<T> {
    #[inline(always)]
    pub const fn empty() -> Self {
        Self {
            ptr: ptr::NonNull::dangling(),
            capacity_bytes: 0,
        }
    }

    #[inline(always)]
    pub const fn as_mut_ptr(&self) -> *mut T {
        self.ptr.as_ptr()
    }

    #[inline(always)]
    pub const fn capacity_bytes(&self) -> usize {
        self.capacity_bytes
    }

    #[inline(always)]
    pub fn try_reserve(&mut self, size: usize) -> Result<(), MemoryError> {
        let align = mem::align_of::<T>();
        let Some(bytes) = size.checked_mul(mem::size_of::<T>()) else {
            return Err(MemoryError);
        };
        let cap = self.capacity_bytes;
        if bytes <= cap {
            return Ok(());
        }
        unsafe {
            let (p2, bytes) = os::realloc(self.ptr.as_ptr() as *mut u8, cap, bytes, align);
            self.ptr = match ptr::NonNull::new(p2 as *mut T) {
                Some(p2) => p2,
                None => return Err(MemoryError),
            };
            self.capacity_bytes = bytes;
        }
        Ok(())
    }

    pub unsafe fn cast<U>(self) -> RawBuf<U> {
        debug_assert!(mem::align_of::<T>() >= mem::align_of::<U>());
        let r = RawBuf {
            ptr: self.ptr.cast(),
            capacity_bytes: self.capacity_bytes,
        };
        mem::forget(self);
        r
    }
}

pub struct Buf<T, const N: usize> {
    raw: RawBuf<T>,
    sizes: [usize; N],
}

impl<T, const N: usize> Drop for Buf<T, N> {
    fn drop(&mut self) {
        unsafe {
            ptr::drop_in_place(ptr::slice_from_raw_parts_mut(
                self.as_mut_ptr(),
                self.total_len(),
            ));
        }
    }
}

impl<T, const N: usize> Default for Buf<T, N> {
    #[inline(always)]
    fn default() -> Self {
        Self {
            raw: RawBuf::empty(),
            sizes: [0; N],
        }
    }
}

impl<T, const N: usize> ops::Index<usize> for Buf<T, N> {
    type Output = [T];
    #[inline(always)]
    fn index(&self, i: usize) -> &[T] {
        let begin = self.sizes[0..i].iter().copied().sum();
        unsafe { slice::from_raw_parts(self.as_mut_ptr().add(begin), self.sizes[i]) }
    }
}

impl<T, const N: usize> ops::IndexMut<usize> for Buf<T, N> {
    #[inline(always)]
    fn index_mut(&mut self, i: usize) -> &mut [T] {
        let begin = self.sizes[0..i].iter().copied().sum();
        unsafe { slice::from_raw_parts_mut(self.as_mut_ptr().add(begin), self.sizes[i]) }
    }
}

impl<T, const N: usize> Buf<T, N> {
    #[inline(always)]
    pub const fn as_mut_ptr(&self) -> *mut T {
        self.raw.as_mut_ptr()
    }

    #[inline(always)]
    pub const fn capacity_bytes(&self) -> usize {
        self.raw.capacity_bytes()
    }

    #[inline(always)]
    pub fn total_len(&self) -> usize {
        let mut r = 0usize;
        for v in self.sizes {
            r += v;
        }
        r
    }

    #[inline(always)]
    pub unsafe fn set_len(&mut self, sizes: [usize; N]) {
        self.sizes = sizes;
    }

    #[inline(always)]
    pub fn extend_empty<const M: usize>(self) -> Buf<T, M> {
        let sizes = self.sizes;
        let raw = self.into_raw();
        let mut r = Buf {
            raw,
            sizes: [0; M],
        };
        let n = N.min(M);
        r.sizes[0..n].copy_from_slice(&sizes[0..n]);
        r
    }

    #[inline(always)]
    pub fn trim_unused(&self) {
        let offset = self.total_len() * mem::size_of::<T>();
        unsafe {
            os::trim(self.as_mut_ptr() as *mut u8, self.capacity_bytes(), offset);
        }
    }

    #[inline(always)]
    pub fn into_raw(mut self) -> RawBuf<T> {
        std::mem::take(&mut self.raw)
    }

    #[inline(always)]
    pub fn iter<'a>(&'a self) -> impl 'a + Iterator<Item = &'a [T]> {
        (0..N).map(|i| &self[i])
    }

    #[inline(always)]
    pub fn single_slice(&self) -> &[T] {
        unsafe { slice::from_raw_parts(self.as_mut_ptr(), self.total_len()) }
    }

    #[inline(always)]
    pub fn slices<'a>(&'a self) -> [&'a [T]; N] {
        let mut ptr = self.as_mut_ptr();
        array::from_fn(|i| unsafe {
            let n = self.sizes[i];
            let r = slice::from_raw_parts(ptr, n);
            ptr = ptr.add(n);
            r
        })
    }

    #[inline(always)]
    pub fn slices_mut<'a>(&'a mut self) -> [&'a mut [T]; N] {
        let mut ptr = self.as_mut_ptr();
        array::from_fn(|i| unsafe {
            let n = self.sizes[i];
            let r = slice::from_raw_parts_mut(ptr, n);
            ptr = ptr.add(n);
            r
        })
    }
}

impl<T, const N: usize> Buf<mem::MaybeUninit<T>, N> {
    #[inline(always)]
    pub fn make_uninit(
        mut raw: RawBuf<T>,
        sizes: [usize; N],
    ) -> Result<Buf<mem::MaybeUninit<T>, N>, MemoryError> {
        let total_len = sizes.iter().sum();
        raw.try_reserve(total_len)?;
        Ok(Buf {
            raw: unsafe { raw.cast() },
            sizes,
        })
    }

    #[inline(always)]
    pub fn new_uninit(sizes: [usize; N]) -> Result<Self, MemoryError> {
        Self::make_uninit(RawBuf::default(), sizes)
    }

    #[inline(always)]
    pub unsafe fn assume_init(self) -> Buf<T, N> {
        let sizes = self.sizes;
        Buf {
            raw: unsafe { self.into_raw().cast() },
            sizes,
        }
    }
}

pub struct WriteChunk<'a, T> {
    marker: PhantomData<&'a mut [mem::MaybeUninit<T>]>,
    ptr: *mut T,
}

impl<'a, T> WriteChunk<'a, T> {
    #[inline(always)]
    pub unsafe fn push_unchecked(&mut self, val: T) {
        let ptr = self.ptr;
        unsafe {
            self.ptr = ptr.add(1);
            ptr.write(val)
        }
    }
}

pub struct WriteBuf<'a, T> {
    marker: PhantomData<&'a mut [mem::MaybeUninit<T>]>,
    ptr: *mut T,
    sizes: &'a [usize],
}

unsafe impl<'a, T: Send> Send for WriteBuf<'a, T> {}

impl<'a, T> Iterator for WriteBuf<'a, T> {
    type Item = WriteChunk<'a, T>;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let (n, rest) = self.sizes.split_first()?;
        self.sizes = rest;
        let n = *n;
        unsafe {
            let ptr = self.ptr;
            self.ptr = ptr.add(n);
            Some(WriteChunk {
                marker: PhantomData,
                ptr,
            })
        }
    }
}

pub struct BufWriter<T, const N: usize> {
    buf: Buf<mem::MaybeUninit<T>, N>,
    chunk_sizes: Vec<usize>,
    ends: [usize; N],
}

impl<T, const N: usize> BufWriter<T, N> {
    pub fn reserve_chunks<I: IntoIterator<Item = usize>>(out: RawBuf<T>, sizes: impl Fn(usize) -> I) -> Result<Self, MemoryError> {
        let mut chunk_sizes = Vec::<usize>::new();
        let mut ends = [0; N];
        let mut part_sizes = [0; N];
        for i in 0..N {
            let base = chunk_sizes.len();
            chunk_sizes.extend(sizes(i));
            part_sizes[i] = chunk_sizes[base..].iter().copied().sum();
            ends[i] = chunk_sizes.len();
        }

        Ok(Self {
            buf: Buf::make_uninit(out, part_sizes)?,
            chunk_sizes,
            ends,
        })
    }

    pub fn start<'a>(&'a mut self) -> [WriteBuf<'a, T>; N] {
        let mut ptr = self.buf.as_mut_ptr();
        array::from_fn(|i| {
            let off0 = if i == 0 { 0 } else { self.ends[i - 1] };
            let off1 = self.ends[i];
            let r = WriteBuf {
                marker: PhantomData,
                ptr: ptr as *mut T,
                sizes: &self.chunk_sizes[off0..off1],
            };
            ptr = unsafe { ptr.add(self.buf[i].len()) };
            r
        })
    }

    pub fn slice_len(&self, i: usize) -> usize {
        let l = if i > 0 { self.ends[i - 1] } else { 0 };
        let r = self.ends[i];
        self.chunk_sizes[l..r].iter().copied().sum()
    }

    #[inline(always)]
    pub unsafe fn finish(self) -> Buf<T, N> {
        unsafe { self.buf.assume_init() }
    }
}
