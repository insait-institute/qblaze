use crate::{Config, Qubit, threading};
use std::{slice, str};

#[derive(Default)]
pub(crate) struct Perf {
    perf_events: Vec<(*const u8, u32, u32)>,
    perf_data: Vec<u64>,
}

pub(crate) struct Context {
    pub pool: threading::Pool,
    pub chunk_size: usize,
    pub multithreading_threshold: usize,
    pub work_item_min_size: usize,
    pub work_item_max_size: usize,
    pub perf: Option<Perf>,
}

impl Context {
    pub fn new(cfg: &Config) -> Self {
        Self {
            pool: threading::Pool::new(cfg.threads),
            chunk_size: cfg.chunk_size,
            multithreading_threshold: cfg.multithreading_threshold,
            work_item_min_size: cfg.work_item_min_size,
            work_item_max_size: cfg.work_item_max_size,
            perf: if cfg.perf_enabled {
                Some(Perf::default())
            } else {
                None
            },
        }
    }

    pub fn config(&self) -> Config {
        Config {
            max_qubit: Qubit::ZERO,
            threads: self.pool.num_threads(),
            chunk_size: self.chunk_size,
            multithreading_threshold: self.multithreading_threshold,
            work_item_min_size: self.work_item_min_size,
            work_item_max_size: self.work_item_max_size,
            perf_enabled: self.perf.is_some(),
        }
    }
}

impl Perf {
    pub fn write(&mut self, event: &'static str, time: u64, data: &[u64]) {
        self.perf_events.push((
            event.as_ptr(),
            event.len().try_into().unwrap_or(u32::MAX),
            data.len().try_into().unwrap(),
        ));
        self.perf_data.push(time);
        self.perf_data.extend(data);
    }

    pub fn iter(&self) -> impl Iterator<Item=(&'static str, &[u64])> {
        let mut data = &self.perf_data[..];
        self.perf_events.iter().copied().map(move |(ptr, len, dlen)| {
            let event = unsafe {
                str::from_utf8_unchecked(slice::from_raw_parts(ptr, len as usize))
            };
            let (ev_data, rest) = data.split_at(dlen as usize + 1);
            data = rest;
            (event, ev_data)
        })
    }

    pub fn clear(&mut self) {
        self.perf_data.clear();
        self.perf_events.clear();
    }
}
