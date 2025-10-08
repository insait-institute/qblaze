// SPDX-License-Identifier: Apache-2.0
use super::State;
use crate::futex;
use std::{hint, sync::atomic::{AtomicU32, Ordering}};

const MAX_SPIN: u32 = 100;

const LOCK_FLAG: u32 = 1;
const CONTEND_FLAG: u32 = 2;

const STOPPING: u32 = 0;
const STATE_IDLE: u32 = 0;
const STATE_LOCKED: u32 = 1;
const STATE_WORK: u32 = 2;
const STATE_CONTENDED: u32 = 3;
const STATE: u32 = 3;

const IDLE_SHIFT: u8 = 2;
const IDLE_1: u32 = 1 << IDLE_SHIFT;

const WAIT_LOCK: u32 = 1;
const WAIT_WORK: u32 = 2;
const WAIT_IDLE: u32 = 4;


// `StateInner` consistss of two low state bits and an idle thread count.
//
// States:
// 0 - unlocked, no work
// 1 - locked, no contention
// 2 - unlocked, has work
// 3 - locked, contended
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
    let mut spin = MAX_SPIN;
    let mut v;
    loop {
        v = state.sync.load(Ordering::Relaxed);
        while v & LOCK_FLAG == 0 {
            let v2 = (v & !STATE) | STATE_LOCKED;
            match state.sync.compare_exchange_weak(v, v2, Ordering::Acquire, Ordering::Relaxed) {
                Ok(_) => return,
                Err(vc) => v = vc,
            }
        }
        if v & CONTEND_FLAG != 0 {
            break;
        }
        if spin == 0 {
            break;
        }
        spin -= 1;
        hint::spin_loop();
    }
    loop {
        if v & STATE != STATE_CONTENDED {
            let v2 = v | STATE_CONTENDED;
            match state.sync.compare_exchange_weak(v, v2, Ordering::Acquire, Ordering::Relaxed) {
                Ok(_) => {
                    if v & LOCK_FLAG == 0 {
                        return;
                    }
                    v = v2;
                }
                Err(vc) => {
                    v = vc;
                    continue;
                }
            }
        }
        debug_assert!(v & STATE == STATE_CONTENDED);
        futex::wait(&state.sync, v, WAIT_LOCK);
        v = state.sync.load(Ordering::Relaxed);
    }
}

#[inline(never)]
pub fn unlock<'a>(state: &'a State, (): LockInner<'a>, nonempty: bool) {
    let mut v = state.sync.load(Ordering::Relaxed);
    debug_assert!(v & LOCK_FLAG != 0);
    v &= !STATE;
    let mut wake = nonempty;
    if nonempty {
        v |= STATE_WORK;
        if v < IDLE_1 {
            wake = false;
        }
    }
    // Make sure nobody sets the contention flag in the middle.
    if state.sync.swap(v, Ordering::Release) & CONTEND_FLAG != 0 {
        if futex::wake_one(&state.sync, WAIT_LOCK) {
            // No need to wake up an idle thread. It will be woken up on
            // unlock.
            return;
        }
    }
    if wake {
        futex::wake_one(&state.sync, WAIT_WORK);
    }
}

#[inline(never)]
pub fn stop(state: &State) {
    let v = state.sync.load(Ordering::Relaxed);
    debug_assert_eq!(v & STATE, STATE_IDLE);
    debug_assert_eq!(v >> IDLE_SHIFT, state.n - 1);
    state.sync.store(STOPPING, Ordering::Release);
    futex::wake_all(&state.sync, WAIT_WORK | WAIT_LOCK);
}

#[inline]
pub fn start_work<'a>(state: &'a State) -> Option<LockInner<'a>> {
    loop {
        let mut v = state.sync.load(Ordering::Relaxed);
        while v & STATE == STATE_WORK {
            let v2 = v - IDLE_1 - STATE_WORK + STATE_LOCKED;
            match state.sync.compare_exchange_weak(v, v2, Ordering::Acquire, Ordering::Relaxed) {
                Ok(_) => return Some(()),
                Err(vc) => v = vc,
            }
        }
        if v == STOPPING {
            break;
        }
        debug_assert!(v >> IDLE_SHIFT >= 1);
        futex::wait(&state.sync, v, WAIT_WORK);
    }
    None
}

#[inline]
pub fn wait_work<'a>(state: &'a State, (): LockInner<'a>) -> Option<LockInner<'a>> {
    let mut v = unlock_idle(state);
    let num_idle = v >> IDLE_SHIFT;
    debug_assert!(num_idle <= state.n);
    if num_idle == state.n {
        // Everyone is idle. Wake up the main thread.
        futex::wake_one(&state.sync, WAIT_IDLE);
    }
    debug_assert_eq!(v & STATE, STATE_IDLE);
    let mut spin = MAX_SPIN;
    loop {
        debug_assert!(v & STATE != STATE_WORK);
        debug_assert!(v >> IDLE_SHIFT >= 1);
        if spin > 0 {
            spin -= 1;
            hint::spin_loop();
        } else {
            futex::wait(&state.sync, v, WAIT_WORK);
        }
        v = state.sync.load(Ordering::Relaxed);
        while v & STATE == STATE_WORK {
            let v2 = v - IDLE_1 - STATE_WORK + STATE_LOCKED;
            match state.sync.compare_exchange_weak(v, v2, Ordering::Acquire, Ordering::Relaxed) {
                Ok(_) => return Some(()),
                Err(vc) => v = vc,
            }
        }
        if v & STATE == STATE_CONTENDED {
            spin = 0;
            continue;
        }
        if v == STOPPING {
            break;
        }
        // locked-uncontended OR unlocked-idle; spin
    }
    None
}

#[inline(never)]
pub fn wait_main<'a>(state: &'a State, (): LockInner<'a>) -> Option<LockInner<'a>> {
    let mut v = unlock_idle(state);
    let mut spin = MAX_SPIN;
    let idle = state.n << IDLE_SHIFT;
    while v != idle {
        debug_assert!(v & STATE != STATE_WORK);
        debug_assert!(v >> IDLE_SHIFT < state.n);
        debug_assert!(v >> IDLE_SHIFT >= 1);
        if spin > 0 {
            spin -= 1;
            hint::spin_loop();
        } else {
            futex::wait(&state.sync, v, WAIT_WORK | WAIT_IDLE);
        }
        v = state.sync.load(Ordering::Relaxed);
        while v & STATE == STATE_WORK {
            let v2 = v - IDLE_1 - STATE_WORK + STATE_LOCKED;
            match state.sync.compare_exchange_weak(v, v2, Ordering::Acquire, Ordering::Relaxed) {
                Ok(_) => return Some(()),
                Err(vc) => v = vc,
            }
        }
        if v & STATE == STATE_CONTENDED {
            spin = 0;
        }
    }
    state.sync.store(v - IDLE_1, Ordering::Relaxed);
    None
}

#[inline(always)]
fn unlock_idle<'a>(state: &'a State) -> u32 {
    let mut v = state.sync.load(Ordering::Relaxed);
    debug_assert!(v & LOCK_FLAG == LOCK_FLAG);
    v = (v & !STATE) + IDLE_1;
    // Swap instead of store to avoid accidentally clearing the contention flag.
    if state.sync.swap(v, Ordering::Release) & CONTEND_FLAG != 0 {
        futex::wake_one(&state.sync, WAIT_LOCK);
    }
    v
}
