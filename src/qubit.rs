// SPDX-License-Identifier: Apache-2.0
use crate::bitset::{BitIndex, BITSET_WORD};
use std::{fmt, hash, mem, slice};

/// Qubit index.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Qubit(u32);

impl Qubit {
    pub const MAX: Qubit = Qubit(62 * 64 - 1);
    pub const ZERO: Qubit = Qubit(0);

    #[inline(always)]
    pub const fn from_index(i: usize) -> Self {
        assert!(i <= Self::MAX.index());
        Self(i as u32)
    }

    #[inline(always)]
    pub const fn try_from_index(i: usize) -> Option<Self> {
        if i > Self::MAX.index() {
            return None;
        }
        Some(Self(i as u32))
    }

    #[inline(always)]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl<const N: usize> From<BitIndex<N>> for Qubit {
    #[inline(always)]
    fn from(v: BitIndex<N>) -> Self {
        let () = const {
            assert!(N * BITSET_WORD <= Self::MAX.index() + 1);
        };
        Self(v.get_u32())
    }
}

impl<const N: usize> From<Qubit> for BitIndex<N> {
    #[inline(always)]
    fn from(v: Qubit) -> Self {
        Self::new_u32(v.0)
    }
}

impl fmt::Debug for Qubit {
    fn fmt(&self, w: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(w, "q{}", self.0)
    }
}

/// One element of a control: pair of (qubit, value).
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct ControlItem(u32);

impl ControlItem {
    #[inline(always)]
    pub const fn new(q: Qubit, v: bool) -> Self {
        Self((q.0 << 1) | (v as u32))
    }

    #[inline(always)]
    pub const fn qubit(self) -> Qubit {
        Qubit(self.0 >> 1)
    }

    #[inline(always)]
    pub const fn value(self) -> bool {
        (self.0 & 1) != 0
    }

    #[inline(always)]
    pub const fn invert(self) -> Self {
        Self(self.0 ^ 1)
    }
}

impl fmt::Debug for ControlItem {
    fn fmt(&self, w: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(w, "q{}={}", self.0 >> 1, ['0', '1'][(self.0 & 1) as usize])
    }
}

/// Sequence of zero or more `ControlItem`s.
pub struct Control {
    val: ControlArray,
    count: usize,
}

#[derive(Copy, Clone)]
union ControlArray {
    empty: (),
    one: ControlItem,
    two: [ControlItem; 2],
    multi: *mut ControlItem,
}

unsafe impl Send for Control {}
unsafe impl Sync for Control {}

impl Control {
    /// The `Control` that is always true.
    #[inline(always)]
    pub const fn always() -> Self {
        Self {
            val: ControlArray { empty: () },
            count: 0,
        }
    }

    /// A `Control` that depends on a single qubit.
    #[inline(always)]
    pub(crate) const fn single(q: Qubit, v: bool) -> Self {
        Self::one(ControlItem::new(q, v))
    }

    #[inline(always)]
    const fn one(item: ControlItem) -> Self {
        Self {
            val: ControlArray { one: item },
            count: 1,
        }
    }

    #[inline(always)]
    const fn two_ordered(a: ControlItem, b: ControlItem) -> Self {
        Self {
            val: ControlArray { two: [a, b] },
            count: 2,
        }
    }

    #[inline]
    fn two(mut a: ControlItem, mut b: ControlItem) -> Self {
        if a > b {
            mem::swap(&mut a, &mut b);
        }
        Self::two_ordered(a, b)
    }

    #[inline]
    fn from_box(b: Box<[ControlItem]>) -> Self {
        let count = b.len();
        debug_assert!(count > 2);
        Self {
            val: ControlArray {
                multi: unsafe { (*Box::into_raw(b)).as_mut_ptr() },
            },
            count,
        }
    }

    #[inline(always)]
    const unsafe fn allocated_get(&self) -> &[ControlItem] {
        unsafe { slice::from_raw_parts(self.val.multi, self.count) }
    }

    #[inline(always)]
    unsafe fn allocated_get_mut(&mut self) -> &mut [ControlItem] {
        unsafe { slice::from_raw_parts_mut(self.val.multi, self.count) }
    }

    /// Get the list of `ControlItem`s.
    #[inline(always)]
    pub(crate) const fn get(&self) -> &[ControlItem] {
        match self.count {
            0 => &[],
            1 => slice::from_ref(unsafe { &self.val.one }),
            2 => unsafe { &self.val.two },
            _ => unsafe { self.allocated_get() },
        }
    }

    fn get_mut(&mut self) -> &mut [ControlItem] {
        match self.count {
            0 => &mut [],
            1 => slice::from_mut(unsafe { &mut self.val.one }),
            2 => unsafe { &mut self.val.two },
            _ => unsafe { self.allocated_get_mut() },
        }
    }

    /// Return `true` if the control is always true.
    #[inline]
    pub const fn is_always(&self) -> bool {
        self.count == 0
    }

    /// Return whether the control depends on `q`.
    pub fn uses(&self, q: Qubit) -> bool {
        self.iter().any(|ctl| ctl.qubit() == q)
    }

    /// Iterate over the list of `ControlItem`s.
    pub fn iter<'a>(&'a self) -> impl 'a + Iterator<Item = ControlItem> {
        self.get().iter().copied()
    }

    pub(crate) fn with_qubit(&self, q: Qubit, v: bool) -> Self {
        let c = ControlItem::new(q, v);
        match *self.get() {
            [] => Self::one(c),
            [c0] => {
                assert!(c0.qubit() != q, "Multiple controls on {q:?}");
                Self::two(c0, c)
            }
            ref items => {
                let Err(i) = items.binary_search_by(|c0| c0.qubit().cmp(&q)) else {
                    panic!("Multiple controls on {q:?}");
                };
                let mut v = Vec::with_capacity(items.len() + 1);
                v.extend_from_slice(&items[..i]);
                v.push(c);
                v.extend_from_slice(&items[i..]);
                Self::from_box(v.into_boxed_slice())
            }
        }
    }

    pub(crate) fn inplace_toggle_if(&mut self, mut f: impl FnMut(Qubit) -> bool) {
        for item in self.get_mut() {
            if f(item.qubit()) {
                *item = item.invert();
            }
        }
    }

    pub(crate) fn without_qubit(&self, q: Qubit) -> Option<(bool, Self)> {
        match self.get() {
            [] => None,
            [c0] => if c0.qubit() == q {
                Some((c0.value(), Self::always()))
            } else {
                None
            }
            items => {
                let i = match items.binary_search_by(|c0| c0.qubit().cmp(&q)) {
                    Err(_) => return None,
                    Ok(i) => i,
                };
                let r = items[..i].iter().copied().chain(items[i+1..].iter().copied()).collect();
                Some((items[i].value(), r))
            }
        }
    }
}

impl fmt::Debug for Control {
    fn fmt(&self, w: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_always() {
            return w.write_str("1");
        }
        let mut first = true;
        for ci in self.iter() {
            if !first {
                w.write_str(",")?;
            }
            first = false;
            <ControlItem as fmt::Debug>::fmt(&ci, w)?;
        }
        Ok(())
    }
}

impl Clone for Control {
    fn clone(&self) -> Self {
        Self {
            val: match self.count {
                0..=2 => self.val,
                _ => ControlArray {
                    multi: unsafe { (*Box::<[ControlItem]>::into_raw(Box::from(self.allocated_get()))).as_mut_ptr() },
                },
            },
            count: self.count,
        }
    }
}

impl Drop for Control {
    fn drop(&mut self) {
        if self.count <= 2 {
            return;
        }
        let _ = unsafe { Box::from_raw(slice::from_raw_parts_mut(self.val.multi, self.count)) };
    }
}

impl Eq for Control {}

impl PartialEq for Control {
    fn eq(&self, other: &Self) -> bool {
        *self.get() == *other.get()
    }
}

impl hash::Hash for Control {
    #[inline]
    fn hash<H: hash::Hasher>(&self, h: &mut H) {
        <[ControlItem] as hash::Hash>::hash(self.get(), h)
    }
}

impl FromIterator<ControlItem> for Control {
    fn from_iter<I: IntoIterator<Item = ControlItem>>(iter: I) -> Self {
        let mut iter = iter.into_iter();
        let Some(first) = iter.next() else {
            return Control::always();
        };
        let Some(second) = iter.next() else {
            return Control::one(first);
        };
        let Some(third) = iter.next() else {
            assert!(first.qubit() != second.qubit(), "Multiple controls on {:?}", first.qubit());
            return Control::two(first, second);
        };
        let mut v = Vec::with_capacity(3 + iter.size_hint().0.min(1));
        v.push(first);
        v.push(second);
        v.push(third);
        v.extend(iter);
        v.sort();
        for w in v.windows(2) {
            if w[0].qubit() == w[1].qubit() {
                panic!("Multiple controls on {:?}", w[0].qubit());
            }
        }
        Self::from_box(v.into_boxed_slice())
    }
}

impl Control {
    /// Create a `Control` from `ControlItem`s. Returns `Ok(None)` if there is aliasing.
    pub fn try_from_iter<E, I: IntoIterator<Item = Result<ControlItem, E>>>(iter: I) -> Result<Option<Self>, E> {
        let mut iter = iter.into_iter();
        let Some(first) = iter.next() else {
            return Ok(Some(Control::always()));
        };
        let first = first?;
        let Some(second) = iter.next() else {
            return Ok(Some(Control::one(first)));
        };
        let second = second?;
        let Some(third) = iter.next() else {
            if first.qubit() == second.qubit() {
                return Ok(None);
            }
            return Ok(Some(Control::two(first, second)));
        };
        let third = third?;
        let mut v = Vec::with_capacity(2 + iter.size_hint().0.min(2));
        v.push(first);
        v.push(second);
        v.push(third);
        for c in iter {
            v.push(c?);
        }
        v.sort();
        for w in v.windows(2) {
            if w[0].qubit() == w[1].qubit() {
                return Ok(None);
            }
        }
        Ok(Some(Self::from_box(v.into_boxed_slice())))
    }
}
