use lazy_static::lazy_static;
use crate::queue::Queue;
use std::collections::HashMap;
use std::marker::Sync;

struct QueuePtr(*mut Queue<u64>);
unsafe impl Sync for QueuePtr {}

// Global queue for single producer, single consumer
lazy_static! {
    static ref SYMBOLS_QUEUE: QueuePtr = {
        let queue = Box::new(Queue::new());
        QueuePtr(Box::into_raw(queue))
    };
}

pub fn produce_symbol(elem: u64) {
    unsafe { (*SYMBOLS_QUEUE.0).produce(Some(elem)) }
}

pub fn consume_symbol() -> Option<u64> {
    unsafe { (*SYMBOLS_QUEUE.0).consume() }
}

pub struct Symbolizer {
    cache: HashMap<u64, bool>,
}

impl Symbolizer {
    pub fn new() -> Self {
        Self { cache: HashMap::new() }
    }

    pub fn push(&mut self, addr: u64) {
        self.cache.entry(addr).or_insert_with(|| {
            produce_symbol(addr);
            true
        });
    }
}
