use libc::{pthread_self, pthread_t};
use rb_sys::{rb_ll2inum, RTypedData, VALUE};
use rbspy_ruby_structs::ruby_3_1_5::{rb_global_vm_lock_t, rb_thread_t};

pub(crate) unsafe extern "C" fn rb_log_gvl_addr(_module: VALUE, thread_val: VALUE) -> VALUE {
    // todo: handle logger initialization
    let thread_ptr: *mut RTypedData = thread_val as *mut RTypedData;
    let rb_thread_ptr = (*thread_ptr).data as *mut rb_thread_t;

    // access gvl_addr through offset directly
    let gvl_addr = (*rb_thread_ptr).ractor as u64 + 344;
    let gvl_ref = gvl_addr as *mut rb_global_vm_lock_t;
    let lock_addr = &((*gvl_ref).lock) as *const _ as u64;
    let tid: pthread_t = pthread_self();

    log::info!(
        "[lock] thread_id={}, rb_thread_addr={}, gvl_mutex_addr={}",
        tid,
        rb_thread_ptr as u64,
        lock_addr
    );

    rb_ll2inum(lock_addr as i64) as VALUE
}
