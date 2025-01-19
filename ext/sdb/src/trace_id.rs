use rb_sys::{rb_num2ulong, Qtrue, VALUE};
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

// The trace_id is set by applications threads and read by stack puller thread.
// They should not block each other's execution.
// Correctness is not our first consideration, we only require hardware can access this atomically.
pub fn get_trace_id_table() -> &'static mut HashMap<u64, u64> {
    unsafe {
        if TRACE_TABLE.is_null() {
            init_trace_id_table();
        }
        &mut *TRACE_TABLE
    }
}

pub(crate) unsafe extern "C" fn rb_set_trace_id(
    _module: VALUE,
    thread: VALUE,
    trace_id: VALUE,
) -> VALUE {
    let trace_table = get_trace_id_table();

    trace_table.insert(thread, rb_num2ulong(trace_id));

    Qtrue as VALUE
}
