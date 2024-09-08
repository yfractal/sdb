extern crate libc;
extern crate libloading;

use libc::pthread_mutex_t;
use libloading::Library;
use std::sync::Once;
static INIT: Once = Once::new();
static mut REAL_PTHREAD_MUTEX_LOCK: Option<unsafe extern "C" fn(*mut pthread_mutex_t) -> i32> = None;

unsafe fn load_real_pthread_mutex_lock() {
    INIT.call_once(|| {
        let lib = Library::new("libpthread.so.0").expect("Failed to load libpthread");
        let func: libloading::Symbol<unsafe extern "C" fn(*mut pthread_mutex_t) -> i32> =
            lib.get(b"pthread_mutex_lock").expect("Failed to load pthread_mutex_lock symbol");
        REAL_PTHREAD_MUTEX_LOCK = Some(*func);
    });
}

#[no_mangle]
pub unsafe extern "C" fn pthread_mutex_lock(mutex: *mut pthread_mutex_t) -> i32 {
    load_real_pthread_mutex_lock();
    print!("pthread_mutex_lock patch\n");
    if let Some(real_pthread_mutex_lock) = REAL_PTHREAD_MUTEX_LOCK {
        real_pthread_mutex_lock(mutex)
    } else {
        eprintln!("Failed to resolve pthread_mutex_lock");
        -1
    }
}
