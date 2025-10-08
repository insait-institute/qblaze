// SPDX-License-Identifier: Apache-2.0
use super::State;
use std::{mem, sync};

const STOPPING: u32 = 1u32 << 31;

pub struct StateInner {
    cond: sync::Condvar,
    mtx: sync::Mutex<u32>,
}

pub type LockInner<'a> = sync::MutexGuard<'a, u32>;

#[inline]
pub fn new(n: u32) -> StateInner {
    StateInner {
        mtx: sync::Mutex::new(n - 1),
        cond: sync::Condvar::new(),
    }
}

#[inline(never)]
pub fn lock<'a>(state: &'a State) -> LockInner<'a> {
    state.sync.mtx.lock().unwrap()
}

#[inline(never)]
pub fn unlock<'a>(state: &'a State, lg: LockInner<'a>, nonempty: bool) {
    let idle = *lg;
    mem::drop(lg);
    if nonempty && idle != 0 {
        state.sync.cond.notify_one();
    }
}

#[inline(never)]
pub fn stop(state: &State) {
    let mut lg = lock(state);
    assert!(*lg == state.n - 1);
    *lg = STOPPING;
    mem::drop(lg);
    state.sync.cond.notify_all();
}

#[inline]
pub fn wait_work<'a>(state: &'a State, mut lg: LockInner<'a>) -> Option<LockInner<'a>> {
    debug_assert!(*lg & STOPPING == 0);
    *lg += 1;
    if *lg == state.n {
        state.sync.cond.notify_all();
    }
    lock_work(state, lg)
}

#[inline]
pub fn start_work<'a>(state: &'a State) -> Option<LockInner<'a>> {
    lock_work(state, state.sync.mtx.lock().unwrap())
}

#[inline(never)]
fn lock_work<'a>(state: &'a State, mut lg: LockInner<'a>) -> Option<LockInner<'a>> {
    while *lg & STOPPING == 0 {
        lg = cond_wait(state, lg);
        if unsafe { !(*state.tasks.get()).is_empty() } {
            *lg -= 1;
            return Some(lg);
        }
    }
    debug_assert!(*lg == STOPPING);
    None
}

#[inline(never)]
pub fn wait_main<'a>(state: &'a State, mut lg: LockInner<'a>) -> Option<LockInner<'a>> {
    debug_assert!(*lg & STOPPING == 0);
    *lg += 1;
    while *lg != state.n {
        debug_assert!(*lg & STOPPING == 0);
        lg = cond_wait(state, lg);
        if unsafe { !(*state.tasks.get()).is_empty() } {
            *lg -= 1;
            return Some(lg);
        }
    }
    *lg = state.n - 1;
    None
}

#[inline(always)]
fn cond_wait<'a>(state: &'a State, lg: LockInner<'a>) -> LockInner<'a> {
    state.sync.cond.wait(lg).unwrap()
}
