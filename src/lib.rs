// SPDX-License-Identifier: Apache-2.0
#![allow(clippy::collapsible_if)]
#![allow(clippy::identity_op)]
#![allow(clippy::needless_lifetimes)]
#![allow(clippy::type_complexity)]

macro_rules! perf {
    ($ctx:ident[$op:literal $(,$args:expr)* $(,)?] { $($code:tt)* }) => {
        {
            #[cfg(feature = "perf")]
            let t0 = ::std::time::Instant::now();
            let r = {
                $($code)*
            };
            #[cfg(feature = "perf")]
            if let Some(perf) = $ctx.perf.as_mut() {
                let t1 = ::std::time::Instant::now();
                #[cfg(feature = "perf")]
                perf.write($op, (t1 - t0).as_nanos() as u64, &[$(u64::try_from($args).unwrap_or(u64::MAX)),*]);
            }
            #[cfg(not(feature = "perf"))]
            if false { let _ = ($(&$args,)*); }
            r
        }
    };

    ($r:ident = $ctx:ident[$op:literal $(,$args:expr)* $(,)?] { $($code:tt)* }) => {
        let $r = {
            #[cfg(feature = "perf")]
            let t0 = ::std::time::Instant::now();
            let $r = {
                $($code)*
            };
            #[cfg(feature = "perf")]
            if let Some(perf) = $ctx.perf.as_mut() {
                let t1 = ::std::time::Instant::now();
                #[cfg(feature = "perf")]
                perf.write($op, (t1 - t0).as_nanos() as u64, &[$(u64::try_from($args).unwrap_or(u64::MAX)),*]);
            }
            #[cfg(not(feature = "perf"))]
            if false { let _ = ($(&$args,)*); }
            $r
        };
    };
}

#[cfg(feature = "trace_match")]
macro_rules! trace_match {
    ($($args:tt)*) => {
        eprintln!($($args)*)
    }
}

#[cfg(not(feature = "trace_match"))]
macro_rules! trace_match {
    ($fmt:literal $(,$args:expr)*) => { if false { let _ = ($(&$args),*); } };
}

#[cfg(feature = "trace_run")]
macro_rules! trace_run {
    ($($args:tt)*) => {
        eprintln!($($args)*)
    }
}

#[cfg(not(feature = "trace_run"))]
macro_rules! trace_run {
    ($fmt:literal $(,$args:expr)*) => { if false { let _ = ($(&$args),*); } };
}

mod util;

#[cfg(all(target_os = "linux", feature = "threading", feature = "sync_futex"))]
mod futex;

mod shamelessly_stolen_from_libstd;

#[cfg_attr(not(feature = "threading"), path="single_threading.rs")]
mod threading;

mod arena;

mod qubit;
pub use qubit::{Control, ControlItem, Qubit};
mod qubit_set;
use qubit_set::QubitSet;
mod angle;
pub use angle::Angle;
mod u3;
pub use u3::U3;
mod complex;
use complex::Complex;
mod u4;
use u4::U4;
mod bitset;
mod sfree;

mod context;
pub(crate) use context::Context;

mod statevector;

mod buffer;

mod config;
pub use config::Config;

mod simulator;
pub use simulator::{Simulator, Iter};

#[cfg(feature = "capi")]
pub mod capi;
