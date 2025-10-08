// SPDX-License-Identifier: Apache-2.0
#![allow(clippy::missing_safety_doc)]
use std::{alloc, cmp, mem, ptr, slice};

pub enum QBlazeSimulator {}

pub type QBlazeComplex = crate::Complex;

pub const QBLAZE_ERR_MEMORY: libc::c_int = -1;
pub const QBLAZE_ERR_DOMAIN: libc::c_int = -2;
pub const QBLAZE_ERR_QUBIT_INDEX: libc::c_int = -3;
pub const QBLAZE_ERR_QUBIT_USAGE: libc::c_int = -4;

pub const QBLAZE_OPT_END: libc::c_int = 0;
pub const QBLAZE_OPT_DUMP_CONFIG: libc::c_int = 1;
pub const QBLAZE_OPT_QUBIT_COUNT: libc::c_int = 2;
pub const QBLAZE_OPT_THREAD_COUNT: libc::c_int = 3;
pub const QBLAZE_OPT_CHUNK_SIZE: libc::c_int = 4;
pub const QBLAZE_OPT_MULTITHREADING_THRESHOLD: libc::c_int = 6;
pub const QBLAZE_OPT_WORK_ITEM_MIN_SIZE: libc::c_int = 7;
pub const QBLAZE_OPT_WORK_ITEM_MAX_SIZE: libc::c_int = 8;
pub const QBLAZE_OPT__PERF_ENABLED: libc::c_int = 9;

#[derive(Copy, Clone)]
#[repr(C)]
pub struct QBlazeConfig {
    pub option: libc::c_int,
    pub value: QBlazeConfigValue,
}

#[derive(Copy, Clone)]
pub union QBlazeConfigValue {
    pub as_size_t: libc::size_t,
    pub as_ptr: *mut (),
}

#[repr(C)]
pub struct QBlazeIter {
    amplitude: QBlazeComplex,
    qubit_count: usize,
    bitmap: [u8; 0],
}

#[repr(C)]
struct QBlazeIterPriv<'a> {
    iter: crate::Iter<'a>,
    head: QBlazeIter,
}

trait AsInt {
    fn value(v: Self) -> libc::c_int;
}

impl AsInt for () {
    #[inline(always)]
    fn value((): ()) -> libc::c_int {
        0
    }
}

impl AsInt for bool {
    #[inline(always)]
    fn value(v: bool) -> libc::c_int {
        v as libc::c_int
    }
}

macro_rules! wrap_err {
    { $sim:ident; $($body:tt)* } => {
        let $sim: &mut crate::Simulator = unsafe { &mut *($sim as *mut crate::Simulator) };
        let r: Result<_, libc::c_int> = (move |$sim: &mut crate::Simulator| Ok({ $($body)* }))($sim);
        match r {
            Ok(v) => {
                if $sim.is_error() {
                    return QBLAZE_ERR_MEMORY;
                }
                AsInt::value(v)
            }
            Err(e) => {
                debug_assert!(e < 0);
                e
            }
        }
    }
}

#[repr(C)]
pub struct QBlazeControl {
    qubit: libc::size_t,
    value: bool,
}

#[inline]
fn parse_qubit(qubit: usize) -> Result<crate::Qubit, libc::c_int> {
    crate::Qubit::try_from_index(qubit).ok_or(QBLAZE_ERR_QUBIT_INDEX)
}

unsafe fn parse_controls(controls: *const QBlazeControl, count: usize) -> Result<crate::Control, libc::c_int> {
    if count == 0 {
        // `slice::from_raw_parts()` wants the base pointer to be non-NULL.
        return Ok(crate::Control::always());
    }

    crate::Control::try_from_iter(
        unsafe { slice::from_raw_parts(controls, count) }.iter().map(
            |ci| parse_qubit(ci.qubit).map(|q| crate::ControlItem::new(q, ci.value))
        )
    ).and_then(|r| r.ok_or(QBLAZE_ERR_QUBIT_USAGE))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn qblaze_max_qubit_count() -> libc::size_t {
    crate::Qubit::MAX.index() + 1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn qblaze_new(mut cfgs: *const QBlazeConfig) -> *mut QBlazeSimulator {
    let mut config = crate::Config::default();
    let mut dump_config: bool = false;

    if !cfgs.is_null() {
        loop {
            unsafe {
                let cfg = *cfgs;
                cfgs = cfgs.add(1);
                match cfg.option {
                    QBLAZE_OPT_END => break,
                    QBLAZE_OPT_DUMP_CONFIG => dump_config = cfg.value.as_size_t != 0,
                    QBLAZE_OPT_QUBIT_COUNT => config.max_qubit = crate::Qubit::try_from_index(cmp::max(1, cfg.value.as_size_t) - 1).unwrap_or(crate::Qubit::MAX),
                    QBLAZE_OPT_THREAD_COUNT => config.threads = cfg.value.as_size_t,
                    QBLAZE_OPT_CHUNK_SIZE => config.chunk_size = cfg.value.as_size_t,
                    QBLAZE_OPT_MULTITHREADING_THRESHOLD => config.multithreading_threshold = cfg.value.as_size_t,
                    QBLAZE_OPT_WORK_ITEM_MIN_SIZE => config.work_item_min_size = cfg.value.as_size_t,
                    QBLAZE_OPT_WORK_ITEM_MAX_SIZE => config.work_item_max_size = cfg.value.as_size_t,
                    QBLAZE_OPT__PERF_ENABLED => config.perf_enabled = cfg.value.as_size_t != 0,
                    _ => {}
                }
            }
        }
    };

    if dump_config {
        eprintln!("Simulator config: {config:?}");
    }

    Box::into_raw(Box::new(crate::Simulator::new(config))) as *mut QBlazeSimulator
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn qblaze_del(sim: *mut QBlazeSimulator) {
    let _ = unsafe { Box::from_raw(sim as *mut crate::Simulator) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn qblaze_clone(sim: *mut QBlazeSimulator) -> *mut QBlazeSimulator {
    let sim = unsafe { &mut *(sim as *mut crate::Simulator) };
    sim.flush();
    if sim.is_error() {
        return ptr::null_mut();
    }
    sim.trim();
    let r = Box::new(sim.clone_flushed());
    if r.is_error() {
        return ptr::null_mut();
    }
    Box::into_raw(r) as *mut QBlazeSimulator
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn qblaze_apply_u3(sim: *mut QBlazeSimulator, target: libc::size_t, theta: f64, phi: f64, lam: f64) -> libc::c_int {
    wrap_err! { sim;
        if !theta.is_finite() || !phi.is_finite() || !lam.is_finite() {
            return Err(QBLAZE_ERR_DOMAIN);
        }
        let target = parse_qubit(target)?;
        let gate = crate::U3::new(
            crate::Angle::from_radians(theta),
            crate::Angle::from_radians(phi),
            crate::Angle::from_radians(lam),
        );
        sim.u3(target, gate);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn qblaze_apply_mcx(sim: *mut QBlazeSimulator, controls: *const QBlazeControl, count: libc::size_t, target: libc::size_t) -> libc::c_int {
    wrap_err! { sim;
        let target = parse_qubit(target)?;
        let ctl = unsafe { parse_controls(controls, count) }?;
        if ctl.uses(target) {
            return Err(QBLAZE_ERR_QUBIT_USAGE);
        }
        sim.ctl_x(&ctl, target);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn qblaze_apply_mcswap(sim: *mut QBlazeSimulator, controls: *const QBlazeControl, count: libc::size_t, target0: libc::size_t, target1: libc::size_t) -> libc::c_int {
    wrap_err! { sim;
        let target0 = parse_qubit(target0)?;
        let target1 = parse_qubit(target1)?;
        let ctl = unsafe { parse_controls(controls, count) }?;
        if ctl.uses(target0) {
            return Err(QBLAZE_ERR_QUBIT_USAGE);
        }
        if ctl.uses(target1) {
            return Err(QBLAZE_ERR_QUBIT_USAGE);
        }
        sim.ctl_swap(&ctl, target0, target1);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn qblaze_apply_mcphase(sim: *mut QBlazeSimulator, controls: *const QBlazeControl, count: libc::size_t, phase: f64) -> libc::c_int {
    wrap_err! { sim;
        if !phase.is_finite() {
            return Err(QBLAZE_ERR_DOMAIN);
        }
        let ctl = unsafe { parse_controls(controls, count) }?;
        sim.ctl_phase(&ctl, crate::Angle::from_radians(phase));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn qblaze_measure(sim: *mut QBlazeSimulator, target: libc::size_t, bias: u64, p0: *mut f64, p1: *mut f64) -> libc::c_int {
    wrap_err! { sim;
        let target = parse_qubit(target)?;
        let (v, r0, r1) = sim.measure(target, bias);
        if !p0.is_null() {
            unsafe { p0.write(r0); }
        }
        if !p1.is_null() {
            unsafe { p1.write(r1); }
        }
        v
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn qblaze_qubit_probs(sim: *mut QBlazeSimulator, target: libc::size_t, p0: *mut f64, p1: *mut f64) -> libc::c_int {
    wrap_err! { sim;
        let target = parse_qubit(target)?;
        let (r0, r1) = sim.qubit_probs(target);
        if !p0.is_null() {
            unsafe { p0.write(r0); }
        }
        if !p1.is_null() {
            unsafe { p1.write(r1); }
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn qblaze_copy_amplitudes(sim: *mut QBlazeSimulator, buffer: *mut QBlazeComplex, length: libc::size_t) -> libc::c_int {
    let sim = unsafe { &mut *(sim as *mut crate::Simulator) };
    sim.flush();
    if sim.is_error() {
        return QBLAZE_ERR_MEMORY;
    }
    sim.trim();
    for (i, v) in sim.iter() {
        if i[1..].iter().any(|m| *m != 0) || i[0] >= (length as u64) {
            continue;
        }
        unsafe { buffer.add(i[0] as usize).write(v); }
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn qblaze_dump(sim: *mut QBlazeSimulator) -> libc::c_int {
    let sim = unsafe { &mut *(sim as *mut crate::Simulator) };
    sim.flush();
    if sim.is_error() {
        return QBLAZE_ERR_MEMORY;
    }
    let mut max: u64 = 1;
    for (i, _) in sim.iter() {
        if i[0] > max {
            max = i[0];
        }
    }
    let n = 64 - max.leading_zeros() as usize;
    for (i, v) in sim.iter() {
        eprintln!("|{1:00$b}\u{27e9} {2}", n, i[0], v);
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn qblaze_flush(sim: *mut QBlazeSimulator) -> libc::c_int {
    wrap_err! { sim;
        sim.flush();
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _qblaze_perf(sim: *mut QBlazeSimulator) -> *mut libc::c_void {
    let sim = unsafe { &mut *(sim as *mut crate::Simulator) };
    let Some(perf) = sim.ctx.perf.as_mut() else {
        return ptr::null_mut();
    };
    // let mut buf = Vec::<u8>::new_in(std::alloc::System);
    let mut buf = Vec::<u8>::new();
    perf.iter().for_each(|(event, data)| {
        buf.extend(event.bytes());
        for v in data {
            buf.push(32);
            {
                use std::io::Write;
                write!(buf, "{v}").unwrap();
            }
        }
        buf.push(10);
    });
    buf.push(0);
    perf.clear();
    Box::into_raw(buf.into_boxed_slice()) as *mut u8 as *mut _
}

#[inline]
fn qblaze_iter_priv_layout(qubit_count: usize) -> alloc::Layout {
    assert!(qubit_count % 64 == 0);
    let words = qubit_count / 64;
    let mut size = mem::offset_of!(QBlazeIterPriv, head.bitmap) + words * size_of::<u64>();
    size = size.max(size_of::<QBlazeIterPriv>());
    alloc::Layout::from_size_align(size, align_of::<QBlazeIterPriv>()).unwrap()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn qblaze_iter_new(sim: *mut QBlazeSimulator) -> *mut QBlazeIter {
    let sim = unsafe { &mut *(sim as *mut crate::Simulator) };
    sim.flush();
    if sim.is_error() {
        return ptr::null_mut();
    }
    sim.trim();

    let qubit_count = sim.max_qubit().index() + 1;
    let qubit_count = qubit_count.div_ceil(64) * 64;

    let layout = qblaze_iter_priv_layout(qubit_count);
    unsafe {
        let iter = alloc::alloc(layout) as *mut QBlazeIterPriv;
        iter.write(QBlazeIterPriv {
            iter: sim.iter(),
            head: QBlazeIter {
                amplitude: QBlazeComplex::ZERO,
                qubit_count,
                bitmap: [],
            },
        });
        &raw mut (*iter).head
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn qblaze_iter_next(head: *mut QBlazeIter) -> bool {
    unsafe {
        let iter = head.byte_sub(mem::offset_of!(QBlazeIterPriv, head)) as *mut QBlazeIterPriv;

        let Some((bits, val)) = (*iter).iter.next() else {
            return false;
        };

        let qubit_count = (*iter).head.qubit_count;

        assert!(qubit_count == 64 * bits.len());
        let bitmap_ptr = (*iter).head.bitmap.as_mut_ptr();
        let bitmap = slice::from_raw_parts_mut(bitmap_ptr, 8 * bits.len());

        (*iter).head.amplitude = val;
        for (i, v) in bits.iter().copied().enumerate() {
            bitmap[8*i..8*(i+1)].copy_from_slice(&v.to_le_bytes());
        }
        true
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn qblaze_iter_del(head: *mut QBlazeIter) {
    unsafe {
        let iter = head.byte_sub(mem::offset_of!(QBlazeIterPriv, head)) as *mut QBlazeIterPriv;
        let qubit_count = (*iter).head.qubit_count;
        let layout = qblaze_iter_priv_layout(qubit_count);
        alloc::dealloc(iter as *mut u8, layout);
    }
}
