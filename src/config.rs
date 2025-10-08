// SPDX-License-Identifier: Apache-2.0
use crate::Qubit;

/// Simulator configuration.
#[derive(Debug)]
#[non_exhaustive]
pub struct Config {
    /// Initial maximum qubit index.
    ///
    /// This option determines how many bits are used to represent qubit indices.
    /// The representation is changed automatically when a qubit with a higher index is used.
    ///
    /// Default: 0.
    pub max_qubit: Qubit,

    /// The number of threads in the thread pool.
    ///
    /// Default: nproc.
    pub threads: usize,

    /// The chunk size for processing superposition-free gates (in bytes).
    ///
    /// If set to SIZE_MAX, then a single chunk is used.
    ///
    /// Default: smallest L2 cache size.
    pub chunk_size: usize,

    /// The state vector size after which multiple threads will be used (in bytes).
    ///
    /// Default: 64 KiB.
    pub multithreading_threshold: usize,

    /// The minimum size of a thread work item (in bytes).
    ///
    /// Use to ensure that the work per item is larger that the communication overhead.
    ///
    /// Default: 1 KiB.
    pub work_item_min_size: usize,

    /// The maximum size of a thread work item (in bytes).
    ///
    /// Use to ensure that there are enough items for threads that finish early.
    ///
    /// Default: 16 MiB.
    pub work_item_max_size: usize,

    /// Enable the collection of performance data.
    ///
    /// Default: false.
    pub perf_enabled: bool,
}

impl Default for Config {
    #[inline(always)]
    fn default() -> Self {
        let mut r = Self {
            max_qubit: crate::Qubit::from_index(0),
            threads: 1,
            chunk_size: 1 << 18,
            multithreading_threshold: 1 << 16,
            work_item_min_size: 1 << 10,
            work_item_max_size: 1 << 24,
            perf_enabled: false,
        };
        config(&mut r);
        r
    }
}

#[allow(unused)]
fn parse_usize(val: &[u8]) -> Option<usize> {
    if !val.is_ascii() {
        return None;
    }
    let val = unsafe { std::str::from_utf8_unchecked(val) };
    val.parse().ok()
}

fn config(config: &mut crate::Config) {
    // silence warning
    let _ = config;

    #[cfg(all(target_os = "linux", feature = "config_linux"))]
    if config_linux(config) {
        return;
    }

    #[cfg(feature = "threading")]
    if let Ok(threads) = std::thread::available_parallelism() {
        config.threads = threads.get();
    }
}

#[cfg(all(target_os = "linux", feature = "config_linux"))]
fn config_linux(config: &mut crate::Config) -> bool {
    use std::mem;
    use std::collections::hash_map::{HashMap, Entry};

    let mut n_threads = 0;

    // id -> (size, cpus)
    let mut l2_caches = HashMap::<usize, (usize, usize)>::new();

    unsafe {
        let mut cpus = mem::MaybeUninit::<libc::cpu_set_t>::uninit();
        if libc::sched_getaffinity(0, size_of::<libc::cpu_set_t>(), cpus.as_mut_ptr()) < 0 {
            return false;
        }
        let cpus = cpus.assume_init_ref();

        let cpus_fd = libc::open(c"/sys/devices/system/cpu".as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC);

        let mut buf = [0u8; 64];

        for cpu_i in 0..(libc::CPU_SETSIZE as usize) {
            if !libc::CPU_ISSET(cpu_i, cpus) {
                continue;
            }
            n_threads += 1;
            if cpus_fd < 0 {
                continue;
            }

            for cache_i in 0.. {
                {
                    use std::io::Write;
                    let mut fmt_buf: &mut [u8] = &mut buf;
                    write!(&mut fmt_buf, "cpu{cpu_i}/cache/index{cache_i}/level\0").unwrap();
                }
                let fd = libc::openat(cpus_fd, buf.as_ptr() as *const libc::c_char, libc::O_RDONLY | libc::O_CLOEXEC);
                if fd < 0 {
                    break;
                }
                let n = libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len());
                libc::close(fd);
                if n != 2 || buf[0..2] != *b"2\n" {
                    continue;
                }

                {
                    use std::io::Write;
                    let mut fmt_buf: &mut [u8] = &mut buf;
                    write!(&mut fmt_buf, "cpu{cpu_i}/cache/index{cache_i}/id\0").unwrap();
                }
                let fd = libc::openat(cpus_fd, buf.as_ptr() as *const libc::c_char, libc::O_RDONLY | libc::O_CLOEXEC);
                if fd < 0 {
                    continue;
                }
                let n = libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len());
                libc::close(fd);
                if n < 2 {
                    continue;
                }
                let n = n as usize;
                if buf[n - 1] != b'\n' {
                    continue;
                }
                let Some(cache_id) = parse_usize(&buf[0..n - 1]) else {
                    continue;
                };

                let ent = match l2_caches.entry(cache_id) {
                    Entry::Occupied(mut ent) => {
                        ent.get_mut().1 += 1;
                        continue;
                    }
                    Entry::Vacant(ent) => ent,
                };

                {
                    use std::io::Write;
                    let mut fmt_buf: &mut [u8] = &mut buf;
                    write!(&mut fmt_buf, "cpu{cpu_i}/cache/index{cache_i}/size\0").unwrap();
                }
                let fd = libc::openat(cpus_fd, buf.as_ptr() as *const libc::c_char, libc::O_RDONLY | libc::O_CLOEXEC);
                if fd < 0 {
                    continue;
                }
                let n = libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len());
                libc::close(fd);
                if n < 3 {
                    continue;
                }
                let n = n as usize;
                if buf[n - 1] != b'\n' || buf[n - 2] != b'K' {
                    continue;
                }

                let Some(size_k) = parse_usize(&buf[0..n - 2]) else {
                    continue;
                };
                ent.insert((size_k.saturating_mul(1024), 1));
            }
        }

        libc::close(cpus_fd);
    }

    if n_threads == 0 {
        return false;
    }

    config.threads = n_threads;

    if let Some(min_l2_size) = l2_caches.values().map(|(size, cores)| size / cores).min() {
        config.chunk_size = min_l2_size;
    }

    true
}
