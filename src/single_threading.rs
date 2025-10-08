// SPDX-License-Identifier: Apache-2.0
use std::marker::PhantomData;

#[derive(Copy, Clone)]
pub struct Scope<'a>(PhantomData<&'a ()>);

impl<'a> Scope<'a> {
    #[inline(always)]
    pub fn single_threaded() -> Self {
        Self(PhantomData)
    }

    #[inline(always)]
    pub fn is_single_threaded(self) -> bool {
        true
    }

    #[inline(always)]
    pub fn submit_one(self, f: impl 'a + Send + FnOnce(Self)) {
        f(self);
    }

    #[inline(always)]
    pub fn for_each<I, F>(self, iter: I, f: F)
    where
        I: 'a + Send + Iterator,
        F: 'a + Send + Clone + FnOnce(I::Item),
    {
        for v in iter {
            f.clone()(v);
        }
    }
}

pub struct Pool;

impl Pool {
    #[inline(always)]
    pub fn new(_n: usize) -> Self {
        Self
    }

    #[inline(always)]
    pub(crate) fn borrow<'a>(&'a mut self) -> PoolRef<'a> {
        PoolRef::single_threaded()
    }
}

pub struct PoolRef<'a>(PhantomData<&'a mut Pool>);

impl<'a> PoolRef<'a> {
    #[inline(always)]
    pub fn single_threaded() -> Self {
        Self(PhantomData)
    }

    #[inline(always)]
    pub fn borrow<'b>(&'b mut self) -> PoolRef<'b> {
        Self::single_threaded()
    }

    #[inline(always)]
    pub fn num_threads(&self) -> usize {
        1
    }

    #[inline(always)]
    pub fn scope<R>(self, closure: impl for<'s> FnOnce(Scope<'s>) -> R) -> R {
        closure(Scope(PhantomData))
    }

    #[inline(always)]
    pub fn map<I, R, F>(self, iter: I, f: F) -> Vec<R>
    where
        I: Send + Iterator,
        R: Send + Sync,
        F: Sync + Fn(I::Item) -> R,
    {
        iter.map(f).collect()
    }
}
