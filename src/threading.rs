// SPDX-License-Identifier: Apache-2.0
#![allow(clippy::let_unit_value)]
use std::{alloc, cell::UnsafeCell, marker::PhantomData, mem, ops, ptr, sync::Arc, thread};

const MAX_THREADS: u32 = 1 << 12;

#[cfg_attr(feature = "sync_spin", path = "threading/sync_impl.spin.rs")]
#[cfg_attr(all(not(feature = "sync_spin"), target_os = "linux", feature = "sync_futex"), path = "threading/sync_impl.futex.rs")]
mod sync_impl;

#[repr(C)]
struct State {
    tasks: UnsafeCell<TaskStack>,
    sync: sync_impl::StateInner,
    n: u32,
}

unsafe impl Send for State {}
unsafe impl Sync for State {}

struct TaskStack {
    top: *mut u8,
    begin: *mut u8,
    end: *mut u8,
}

type RunT = for<'a> unsafe fn(Lock<'a>) -> Lock<'a>;

impl TaskStack {
    const MIN_ALIGN: usize = mem::align_of::<RunT>();
    const ALLOC_ALIGN: usize = if Self::MIN_ALIGN <= 16 { 64 } else { 4 * Self::MIN_ALIGN };
    const HEADER_SIZE: usize = mem::size_of::<RunT>();
    const EXT_HEADER_SIZE: usize = mem::size_of::<usize>() + Self::HEADER_SIZE;

    #[inline(always)]
    pub fn new() -> Self {
        Self {
            top: ptr::null_mut(),
            begin: ptr::null_mut(),
            end: ptr::null_mut(),
        }
    }

    #[cold]
    unsafe fn realloc(&mut self, want_end: *mut u8) -> *mut u8 {
        unsafe {
            let want_size = want_end.offset_from(self.begin) as usize;
            let cur_size = self.end.offset_from(self.begin) as usize;
            let mut alloc_size = (cur_size + (cur_size >> 1)).max(want_size);
            alloc_size += alloc_size.wrapping_neg() & (Self::ALLOC_ALIGN - 1);
            let new_begin = if self.begin.is_null() {
                debug_assert!(self.end.is_null());
                debug_assert!(cur_size == 0);
                alloc::alloc(alloc::Layout::from_size_align_unchecked(alloc_size, Self::ALLOC_ALIGN))
            } else {
                alloc::realloc(self.begin, alloc::Layout::from_size_align_unchecked(cur_size, Self::ALLOC_ALIGN), alloc_size)
            };
            self.begin = new_begin;
            self.end = new_begin.add(alloc_size);
            new_begin.add(want_size)
        }
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.top == self.begin
    }

    #[inline(always)]
    pub fn top_func(&self) -> Option<RunT> {
        let top = self.top;
        if top == self.begin {
            return None;
        }
        unsafe {
            let ptr = top.sub(Self::HEADER_SIZE) as *mut RunT;
            let func = *ptr;
            Some(func)
        }
    }

    #[inline(always)]
    unsafe fn top_val<T>(&self) -> *mut T {
        unsafe {
            self.top.sub(Self::full_size::<T>()) as *mut T
        }
    }

    const fn is_overaligned<T>() -> bool {
        mem::align_of::<T>() > TaskStack::MIN_ALIGN
    }

    const fn full_size<T>() -> usize {
        if Self::is_overaligned::<T>() {
            mem::size_of::<T>() + Self::EXT_HEADER_SIZE
        } else {
            mem::size_of::<T>().div_ceil(Self::MIN_ALIGN) * Self::MIN_ALIGN + Self::HEADER_SIZE
        }
    }

    #[inline(always)]
    unsafe fn pop<T: Task>(&mut self) {
        unsafe {
            let bump_size = if Self::is_overaligned::<T>() {
                *(self.top.sub(Self::EXT_HEADER_SIZE) as *mut usize)
            } else {
                Self::full_size::<T>()
            };
            self.top = self.top.sub(bump_size);
        }
    }

    #[inline]
    pub fn push<T: Task>(&mut self, task: T) {
        let mut ptr = self.top;
        unsafe {
            let full_size = Self::full_size::<T>();
            let mut bump_size = full_size;
            if Self::is_overaligned::<T>() {
                assert!(mem::align_of::<T>() <= Self::ALLOC_ALIGN);
                let delta = ptr.align_offset(mem::align_of::<T>());
                bump_size += delta;
                ptr = ptr.add(delta);
            }
            let mut new_top = ptr.add(full_size);
            if (self.end as isize).wrapping_sub(new_top as isize) < 0 {
                new_top = self.realloc(new_top);
                ptr = new_top.sub(full_size);
            }
            (ptr as *mut T).write(task);
            let func = Self::runner::<T>;
            (new_top.sub(Self::HEADER_SIZE) as *mut RunT).write(func);
            if Self::is_overaligned::<T>() {
                (new_top.sub(Self::EXT_HEADER_SIZE) as *mut usize).write(bump_size);
            }
            self.top = new_top;
        }
    }

    #[inline(never)]
    unsafe fn runner<'a, T: Task>(lock: Lock<'a>) -> Lock<'a> {
        T::run(TaskRef { lock, marker: PhantomData })
    }
}

impl Drop for TaskStack {
    fn drop(&mut self) {
        if self.begin.is_null() {
            return;
        }
        assert!(self.is_empty());
        unsafe {
            let size = self.end.offset_from(self.begin) as usize;
            alloc::dealloc(self.begin, alloc::Layout::from_size_align_unchecked(size, Self::ALLOC_ALIGN));
        }
    }
}

struct Lock<'a> {
    state: &'a State,
    sync: mem::ManuallyDrop<sync_impl::LockInner<'a>>,
}

impl<'a> Lock<'a> {
    #[inline(always)]
    fn tasks(&self) -> &TaskStack {
        unsafe { &*self.state.tasks.get() }
    }

    #[inline(always)]
    fn tasks_mut(&mut self) -> &mut TaskStack {
        unsafe { &mut *self.state.tasks.get() }
    }

    fn submit<T: Task>(mut self, task: T) {
        self.tasks_mut().push(task);
        let state = self.state;
        let lck = unsafe { mem::ManuallyDrop::take(&mut self.sync) };
        mem::forget(self);
        sync_impl::unlock(state, lck, true);
    }

    fn run_all(self) -> Self {
        let mut lock = self;
        loop {
            let tasks = lock.tasks_mut();
            let Some(func) = tasks.top_func() else {
                break;
            };
            unsafe {
                lock = func(lock);
            }
        }
        lock
    }

    #[inline]
    fn wait_work(mut self) -> Option<Self> {
        debug_assert!(self.tasks().is_empty());
        let state = self.state;
        let sync = unsafe { mem::ManuallyDrop::take(&mut self.sync) };
        mem::forget(self);
        let sync = sync_impl::wait_work(state, sync)?;
        Some(Self { state, sync: mem::ManuallyDrop::new(sync) })
    }

    #[inline]
    fn wait_main(mut self) -> Option<Self> {
        debug_assert!(self.tasks().is_empty());
        let state = self.state;
        let sync = unsafe { mem::ManuallyDrop::take(&mut self.sync) };
        mem::forget(self);
        let sync = sync_impl::wait_main(state, sync)?;
        Some(Self { state, sync: mem::ManuallyDrop::new(sync) })
    }
}

impl<'a> Drop for Lock<'a> {
    #[inline]
    fn drop(&mut self) {
        let nonempty = !self.tasks().is_empty();
        let state = self.state;
        let sync = unsafe { mem::ManuallyDrop::take(&mut self.sync) };
        sync_impl::unlock(state, sync, nonempty);
    }
}

struct TaskRef<'a, T> {
    lock: Lock<'a>,
    marker: PhantomData<Box<T>>,
}

impl<'a, T: Task> TaskRef<'a, T> {
    #[inline(always)]
    fn state(&self) -> &'a State {
        self.lock.state
    }

    #[inline(always)]
    fn discard(self) -> Lock<'a> {
        let Self { mut lock, .. } = self;
        let tasks = lock.tasks_mut();
        unsafe {
            let ptr = tasks.top_val::<T>();
            tasks.pop::<T>();
            ptr::drop_in_place(ptr);
            lock
        }
    }

    #[inline(always)]
    fn consume(self) -> T {
        let Self { mut lock, .. } = self;
        let tasks = lock.tasks_mut();
        unsafe {
            let ptr = tasks.top_val::<T>();
            tasks.pop::<T>();
            let r = ptr.read();
            mem::drop(lock);
            r
        }
    }
}

impl<'a, T> ops::Deref for TaskRef<'a, T> {
    type Target = T;
    #[inline(always)]
    fn deref(&self) -> &T {
        let tasks = self.lock.tasks();
        unsafe { &*tasks.top_val::<T>() }
    }
}

impl<'a, T> ops::DerefMut for TaskRef<'a, T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut T {
        let tasks = self.lock.tasks_mut();
        unsafe { &mut *tasks.top_val::<T>() }
    }
}

trait Task: Send + Sized {
    fn run<'a>(this: TaskRef<'a, Self>) -> Lock<'a>;
}

#[derive(Copy, Clone)]
pub struct Scope<'a> {
    state: Option<&'a State>,
    marker: PhantomData<fn(&'a mut u8)>,
}

impl<'a> Scope<'a> {
    #[inline(always)]
    unsafe fn from_state<'b>(state: &'b State) -> Self {
        unsafe {
            let state = mem::transmute::<&'b State, &'a State>(state);
            Self { state: Some(state), marker: PhantomData }
        }
    }

    #[inline(always)]
    pub fn single_threaded() -> Self {
        Self { state: None, marker: PhantomData }
    }

    #[inline(always)]
    pub fn is_single_threaded(self) -> bool {
        self.state.is_none()
    }

    pub fn submit_one(self, f: impl 'a + Send + FnOnce(Self)) {
        let Some(state) = self.state else {
            f(self);
            return;
        };
        state.submit(SimpleTask(f));
    }

    fn submit_multi<F, G>(self, mut g: G)
    where
        F: 'a + FnOnce(Scope<'a>),
        G: 'a + Send + FnMut(Scope<'a>) -> Option<F>,
    {
        let Some(state) = self.state else {
            while let Some(f) = g(Self::single_threaded()) {
                f(Self::single_threaded());
            }
            return;
        };
        state.submit(MultiTask(g, PhantomData));
    }

    #[inline(always)]
    pub fn for_each<I, F>(self, mut iter: I, f: F)
    where
        I: 'a + Send + Iterator,
        F: 'a + Send + Clone + FnOnce(I::Item),
    {
        self.submit_multi(move |_| {
            let v = iter.next()?;
            let f = f.clone();
            Some(move |_sc| f(v))
        });
    }
}

impl State {
    #[inline]
    fn lock<'a>(&'a self) -> Lock<'a> {
        let sync = sync_impl::lock(self);
        Lock {
            state: self,
            sync: mem::ManuallyDrop::new(sync),
        }
    }

    #[inline]
    fn start_work<'a>(&'a self) -> Option<Lock<'a>> {
        let sync = sync_impl::start_work(self)?;
        Some(Lock {
            state: self,
            sync: mem::ManuallyDrop::new(sync),
        })
    }

    fn stop(&self) {
        sync_impl::stop(self);
        let tasks = unsafe { &*self.tasks.get() };
        assert!(tasks.is_empty());
    }

    #[inline(always)]
    fn submit<T: Task>(&self, task: T) {
        self.lock().submit(task);
    }

    fn main(&self) {
        let mut lock = self.lock();
        loop {
            lock = lock.run_all();
            match lock.wait_main() {
                Some(l) => lock = l,
                None => break,
            };
        }
    }

    fn work(&self) {
        let Some(mut lock) = self.start_work() else { return };
        loop {
            lock = lock.run_all();
            match lock.wait_work() {
                Some(l) => lock = l,
                None => break,
            };
        }
    }
}

fn iter_size(iter: &impl Iterator) -> (usize, bool) {
    let (min, max) = iter.size_hint();
    if let Some(max) = max {
        return (max, true);
    }
    (min.max(1), false)
}

pub struct Pool {
    state: Option<Arc<State>>,
}

impl Drop for Pool {
    fn drop(&mut self) {
        if let Some(state) = &self.state {
            state.stop();
        }
    }
}

impl Pool {
    pub fn new(mut n: usize) -> Self {
        if n == 0 {
            n = thread::available_parallelism().unwrap().get();
        }
        assert!(n > 0);
        if n == 1 {
            return Self { state: None };
        }
        let n = n.try_into().map(|v: u32| v.min(MAX_THREADS)).unwrap_or(MAX_THREADS);
        let state = Arc::new(State {
            n,
            sync: sync_impl::new(n),
            tasks: UnsafeCell::new(TaskStack::new()),
        });
        for _ in 1..n {
            let state: Arc<State> = state.clone();
            thread::spawn(move || {
                state.work();
                mem::drop(state);
            });
        }
        Self { state: Some(state) }
    }

    #[inline(always)]
    pub(crate) fn borrow<'a>(&'a mut self) -> PoolRef<'a> {
        PoolRef {
            state: self.state.as_deref(),
        }
    }

    #[inline(always)]
    pub(crate) fn maybe_borrow<'a>(&'a mut self, really: bool) -> PoolRef<'a> {
        PoolRef {
            state: if really { self.state.as_deref() } else { None },
        }
    }

    #[inline(always)]
    pub fn num_threads(&self) -> usize {
        match &self.state {
            Some(st) => st.n as usize,
            None => 1,
        }
    }
}

pub struct PoolRef<'a> {
    state: Option<&'a State>,
}

impl<'a> PoolRef<'a> {
    #[inline(always)]
    pub fn borrow<'b>(&'b mut self) -> PoolRef<'b> {
        Self {
            state: self.state,
        }
    }

    pub fn scope<R>(self, closure: impl FnOnce(Scope<'a>) -> R) -> R {
        // We make sure the state is empty.
        let r = closure(Scope { state: self.state, marker: PhantomData });
        if let Some(state) = self.state {
            state.main();
        }
        r
    }

    pub fn map<I, R, F>(mut self, iter: I, f: F) -> Vec<R>
    where
        I: Send + Iterator,
        R: Send + Sync,
        F: Sync + Fn(I::Item) -> R,
    {
        let mut out = Vec::new();

        // Wrapper needs to be Send
        let mut state = MapState {
            iter,
            out: ptr::null_mut(),
            end: ptr::null_mut(),
            more: false,
        };
        loop {
            let (n, upper) = iter_size(&state.iter);
            if upper && n <= 1 {
                out.extend(state.iter.map(f));
                break;
            }
            out.reserve(n);
            state.out = unsafe { out.as_mut_ptr().add(out.len()) };
            state.end = unsafe { out.as_mut_ptr().add(out.capacity()) };
            self.borrow().scope(|sc| sc.submit_multi(|_| {
                let state = &mut state;
                let p = state.out;
                if p == state.end {
                    state.more = state.iter.size_hint().1 != Some(0);
                    return None;
                }
                let v = state.iter.next()?;
                let f = &f;
                state.out = unsafe { p.add(1) };
                Some(move |_| {
                    let v = f(v);
                    unsafe { p.write(v) };
                })
            }));
            unsafe {
                let n = state.out.offset_from(out.as_mut_ptr()) as usize;
                out.set_len(n);
            }
            if !state.more {
                break;
            }
        }

        out
    }
}

struct SimpleTask<F>(F);

impl<'a, F: 'a + Send + FnOnce(Scope<'a>)> Task for SimpleTask<F> {
    fn run(this: TaskRef<Self>) -> Lock {
        let state = this.state();
        let Self(f) = TaskRef::consume(this);
        f(unsafe { Scope::from_state(state) });
        state.lock()
    }
}

struct MultiTask<F, G>(G, PhantomData<fn() -> F>);

impl<'a, F, G> Task for MultiTask<F, G>
where
    F: 'a + FnOnce(Scope<'a>),
    G: 'a + Send + FnMut(Scope<'a>) -> Option<F>,
{
    fn run(mut this: TaskRef<Self>) -> Lock {
        let state = this.state();
        let Some(f) = (this.0)(unsafe { Scope::from_state(state) }) else {
            return TaskRef::discard(this);
        };
        mem::drop(this);
        f(unsafe { Scope::from_state(state) });
        state.lock()
    }
}

struct MapState<I, T> {
    iter: I,
    out: *mut T,
    end: *mut T,
    more: bool,
}

unsafe impl<I: Send, T: Send> Send for MapState<I, T> {}
