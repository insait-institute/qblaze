// SPDX-License-Identifier: Apache-2.0
use super::State;
use std::{hint, sync::atomic::{AtomicU32, Ordering}};

const STOPPING: u32 = 0;
const STATE_IDLE: u32 = 0;
const STATE_LOCKED: u32 = 1;
const STATE_WORK: u32 = 2;
const STATE: u32 = 3;

const IDLE_SHIFT: u8 = 2;
const IDLE_1: u32 = 1 << IDLE_SHIFT;


// `StateInner` consistss of two low state bits and an idle thread count.
//
// States:
// 0 - unlocked, no work
// 1 - locked
// 2 - unlocked, has work
//
// Invariants:
// - An idle thread does not hold the lock.
// - The idle count only changes on lock/unlock or when exiting the main loop.

pub type StateInner = AtomicU32;
pub type LockInner<'a> = ();

#[inline]
pub fn new(n: u32) -> StateInner {
    StateInner::new((n - 1) << IDLE_SHIFT)
}

#[inline(never)]
pub fn lock<'a>(state: &'a State) -> LockInner<'a> {
    loop {
        let mut v = state.sync.load(Ordering::Relaxed);
        while v & STATE_LOCKED == 0 {
            let v2 = (v & !STATE) | STATE_LOCKED;
            match state.sync.compare_exchange_weak(v, v2, Ordering::Acquire, Ordering::Relaxed) {
                Ok(_) => return,
                Err(vc) => v = vc,
            }
        }
        hint::spin_loop();
    }
}

#[inline(never)]
pub fn unlock<'a>(state: &'a State, (): LockInner<'a>, nonempty: bool) {
    let v = state.sync.load(Ordering::Relaxed);
    debug_assert!(v & STATE == STATE_LOCKED);
    let mut v2 = v ^ STATE_LOCKED;
    if nonempty {
        v2 ^= STATE_WORK;
    }
    state.sync.store(v2, Ordering::Release);
}

#[inline(never)]
pub fn stop(state: &State) {
    let v = state.sync.load(Ordering::Relaxed);
    debug_assert_eq!(v & STATE, STATE_IDLE);
    debug_assert_eq!(v >> IDLE_SHIFT, state.n - 1);
    state.sync.store(STOPPING, Ordering::Release);
}

#[inline]
pub fn start_work<'a>(state: &'a State) -> Option<LockInner<'a>> {
    loop {
        let mut v = state.sync.load(Ordering::Relaxed);
        while v & STATE_WORK != 0 {
            let v2 = v - IDLE_1 - STATE_WORK + STATE_LOCKED;
            match state.sync.compare_exchange_weak(v, v2, Ordering::Acquire, Ordering::Relaxed) {
                Ok(_) => return Some(()),
                Err(vc) => v = vc,
            }
        }
        if v == STOPPING {
            return None;
        }
        debug_assert!(v >> IDLE_SHIFT >= 1);
        hint::spin_loop();
    }
}

#[inline]
pub fn wait_work<'a>(state: &'a State, (): LockInner<'a>) -> Option<LockInner<'a>> {
    let mut v = unlock_idle(state);
    debug_assert!(v & STATE == STATE_IDLE);
    loop {
        debug_assert!(v & STATE != STATE_WORK);
        debug_assert!(v >> IDLE_SHIFT >= 1);
        hint::spin_loop();
        v = state.sync.load(Ordering::Relaxed);
        while v & STATE_WORK != 0 {
            let v2 = v - IDLE_1 - STATE_WORK + STATE_LOCKED;
            match state.sync.compare_exchange_weak(v, v2, Ordering::Acquire, Ordering::Relaxed) {
                Ok(_) => return Some(()),
                Err(vc) => v = vc,
            }
        }
        if v == STOPPING {
            return None;
        }
    }
}

#[inline]
pub fn wait_main<'a>(state: &'a State, (): LockInner<'a>) -> Option<LockInner<'a>> {
    let mut v = unlock_idle(state);
    let idle = state.n << IDLE_SHIFT;
    while v != idle {
        debug_assert!(v & STATE != STATE_WORK);
        debug_assert!(v >> IDLE_SHIFT < state.n);
        debug_assert!(v >> IDLE_SHIFT >= 1);
        hint::spin_loop();
        v = state.sync.load(Ordering::Relaxed);
        while v & STATE_WORK != 0 {
            let v2 = v - IDLE_1 - STATE_WORK + STATE_LOCKED;
            match state.sync.compare_exchange_weak(v, v2, Ordering::Acquire, Ordering::Relaxed) {
                Ok(_) => return Some(()),
                Err(vc) => v = vc,
            }
        }
    }
    state.sync.store(v - IDLE_1, Ordering::Relaxed);
    None
}

#[inline(always)]
fn unlock_idle<'a>(state: &'a State) -> u32 {
    let mut v = state.sync.load(Ordering::Relaxed);
    debug_assert!(v & STATE == STATE_LOCKED);
    v = v - STATE_LOCKED + IDLE_1;
    state.sync.store(v, Ordering::Release);
    v
}
