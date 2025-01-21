use rb_sys::{rb_num2ulong, Qfalse, Qtrue, VALUE};
use std::collections::HashMap;
use std::ptr;

static mut TRACE_TABLE: *mut HashMap<u64, u64> = ptr::null_mut();

fn init_trace_id_table() {
    unsafe {
        if TRACE_TABLE.is_null() {
            let map = Box::new(HashMap::new());
            TRACE_TABLE = Box::into_raw(map);
        }
    }
}

// Safety Argument:

// If a hash-map has a fixed size, it's relatively "safe" to access it without a lock.
// Only during rehashing, it needs to avoid all reads at the same time.

// When the Ruby VM creates a new thread, SDB inserts a dummy value into the trace-id table.
// At this moment, it already acquired the THREADS_TO_SCAN_LOCK, which blocks the scanner thread -- the only reader(see rb_add_thread_to_scan method).
// This guarantees that no reader is accessing this table during rehashing.

// Additionally, when SDB needs to read this, it uses a memory barrier for getting the newest value.
// Therefore, I believe this implementation is safe even though it has a lot of "unsafe" code. Yes, it is tricky.
#[inline]
pub fn get_trace_id_table() -> &'static mut HashMap<u64, u64> {
    unsafe {
        if TRACE_TABLE.is_null() {
            init_trace_id_table();
        }

        &mut *TRACE_TABLE
    }
}

#[inline]
pub(crate) unsafe extern "C" fn set_trace_id(thread: VALUE, trace_id: u64) -> bool {
    let trace_table = get_trace_id_table();

    trace_table.insert(thread, trace_id);

    true
}

pub(crate) unsafe extern "C" fn rb_set_trace_id(
    _module: VALUE,
    thread: VALUE,
    trace_id: VALUE,
) -> VALUE {
    if set_trace_id(thread, rb_num2ulong(trace_id)) {
        Qtrue as VALUE
    } else {
        Qfalse as VALUE
    }
}
