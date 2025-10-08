// SPDX-License-Identifier: Apache-2.0
use crate::Qubit;
use std::{fmt, num, slice};

pub struct QubitSet {
    n_words: num::NonZeroUsize,
    data: QubitSetData,
}

unsafe impl Send for QubitSet {}
unsafe impl Sync for QubitSet {}

union QubitSetData {
    single: [u64; 1],
    multi: *mut u64,
}

impl QubitSet {
    #[inline(always)]
    pub fn new() -> Self {
        Self {
            n_words: num::NonZeroUsize::new(1).unwrap(),
            data: QubitSetData {
                single: [0],
            }
        }
    }

    #[inline(always)]
    fn words(&self) -> &[u64] {
        match self.n_words.get() {
            1 => unsafe { &self.data.single },
            n => unsafe { slice::from_raw_parts(self.data.multi, n) },
        }
    }

    #[inline(always)]
    fn words_mut(&mut self) -> &mut [u64] {
        match self.n_words.get() {
            1 => unsafe { &mut self.data.single },
            n => unsafe { slice::from_raw_parts_mut(self.data.multi, n) },
        }
    }

    #[cold]
    fn grow(&mut self, new_n: usize) {
        assert!(new_n >= 2);
        let r = match self.n_words.get() {
            1 => {
                let [v] = unsafe { self.data.single };
                let mut r = vec![0; new_n];
                r[0] = v;
                r
            }
            old_n => {
                let mut r = unsafe { Vec::from_raw_parts(self.data.multi, old_n, old_n) };
                r.resize(new_n, 0);
                r
            }
        };
        self.n_words = num::NonZeroUsize::new(r.len()).unwrap();
        self.data.multi = Box::into_raw(r.into_boxed_slice()) as *mut u64;
    }

    pub fn len(&self) -> usize {
        let mut r = 0;
        for v in self.words() {
            r += v.count_ones() as usize;
        }
        r
    }

    #[inline]
    pub fn contains(&self, q: Qubit) -> bool {
        let q = q.index();
        let i = q >> 6;
        let j = q & 63;
        match self.words().get(i) {
            None => false,
            Some(v) => v & (1u64 << j) != 0,
        }
    }

    #[inline]
    pub fn remove(&mut self, q: Qubit) -> bool {
        let q = q.index();
        let i = q >> 6;
        let j = q & 63;
        match self.words_mut().get_mut(i) {
            None => false,
            Some(p) => {
                let v = *p;
                *p = v & !(1u64 << j);
                v & (1u64 << j) != 0
            }
        }
    }

    #[inline]
    pub fn clear(&mut self) {
        for w in self.words_mut() {
            *w = 0;
        }
    }

    #[inline]
    pub fn insert(&mut self, q: Qubit) {
        let q = q.index();
        let i = q >> 6;
        let j = q & 63;
        if self.words().len() <= i {
            self.grow(i + 1);
        }
        self.words_mut()[i] |= 1u64 << j;
    }

    #[inline]
    pub fn extend(&mut self, qs: impl IntoIterator<Item = Qubit>) {
        for q in qs {
            self.insert(q);
        }
    }

    #[inline(always)]
    pub fn iter<'a>(&'a self) -> impl 'a + Iterator<Item = Qubit> {
        let w = self.words();
        QubitSetIter(w, 0, w[0])
    }

    pub fn intersection(&self, other: &Self) -> Self {
        let w0 = self.words();
        let w1 = other.words();
        let n = w0.len().min(w1.len());
        let mut r = Self::new();
        if n > r.words_mut().len() {
            r.grow(n);
        }
        let w = r.words_mut();
        for i in 0..n {
            w[i] = w0[i] & w1[i];
        }
        r
    }
}

impl Default for QubitSet {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for QubitSet {
    fn clone(&self) -> Self {
        Self {
            n_words: self.n_words,
            data: match self.n_words.get() {
                1 => QubitSetData {
                    single: unsafe { self.data.single },
                },
                _ => QubitSetData {
                    multi: Box::into_raw(Box::<[u64]>::from(self.words())) as *mut u64,
                },
            }
        }
    }
}

impl Drop for QubitSet {
    fn drop(&mut self) {
        match self.n_words.get() {
            1 => {}
            n => unsafe {
                // deallocate
                Vec::from_raw_parts(self.data.multi, n, n);
            }
        };
    }
}

impl fmt::Debug for QubitSet {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("QubitSet(")?;
        let mut first = true;
        for q in self.iter() {
            if !first {
                f.write_str(",")?;
            }
            first = false;
            <Qubit as fmt::Debug>::fmt(&q, f)?;
        }
        f.write_str(")")
    }
}

struct QubitSetIter<'a>(&'a [u64], usize, u64);

impl<'a> Iterator for QubitSetIter<'a> {
    type Item = Qubit;

    #[inline]
    fn next(&mut self) -> Option<Qubit> {
        let mut i = self.1;
        let mut v = self.2;
        while v == 0 {
            i += 1;
            v = *self.0.get(i)?;
            self.1 = i;
        }
        let j = v.trailing_zeros() as usize;
        self.2 = v & (v - 1);
        Some(Qubit::from_index((i << 6) | j))
    }
}
