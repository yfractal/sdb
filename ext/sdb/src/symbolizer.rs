use crate::queue::Queue;
use lazy_static::lazy_static;
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
    unsafe {
        (*SYMBOLS_QUEUE.0).produce(Some(elem));
    }
}

pub fn consume_symbol() -> Option<u64> {
    unsafe { (*SYMBOLS_QUEUE.0).consume() }
}
