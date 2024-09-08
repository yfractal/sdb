extern crate libc;
extern crate libloading;

use libc::pthread_mutex_t;
use libc::{pthread_self, pthread_t};
use libloading::Library;
use std::sync::Once;

static INIT: Once = Once::new();
static mut REAL_PTHREAD_MUTEX_LOCK: Option<unsafe extern "C" fn(*mut pthread_mutex_t) -> i32> =
    None;
static mut REAL_PTHREAD_MUTEX_UNLOCK: Option<unsafe extern "C" fn(*mut pthread_mutex_t) -> i32> =
    None;

use fast_log::config::Config;

unsafe fn init() {
    INIT.call_once(|| {
        fast_log::init(Config::new().file("lock.log").chan_len(Some(1_000_000))).unwrap();

        let lib = Library::new("libpthread.so.0").expect("Failed to load libpthread");

        let lock_func: libloading::Symbol<unsafe extern "C" fn(*mut pthread_mutex_t) -> i32> = lib
            .get(b"pthread_mutex_lock")
            .expect("Failed to load pthread_mutex_lock symbol");
        REAL_PTHREAD_MUTEX_LOCK = Some(*lock_func);

        let unlock_func: libloading::Symbol<unsafe extern "C" fn(*mut pthread_mutex_t) -> i32> =
            lib.get(b"pthread_mutex_unlock")
                .expect("Failed to load pthread_mutex_lock symbol");
        REAL_PTHREAD_MUTEX_UNLOCK = Some(*unlock_func);
    });
}

#[no_mangle]
pub unsafe extern "C" fn pthread_mutex_lock(mutex: *mut pthread_mutex_t) -> i32 {
    init();

    let tid: pthread_t = pthread_self();
    log::info!(
        "[lock][acquire]: thread={}, lock_addr={}",
        tid,
        mutex as u64
    );

    if let Some(real_pthread_mutex_lock) = REAL_PTHREAD_MUTEX_LOCK {
        let ret = real_pthread_mutex_lock(mutex);
        log::info!(
            "[lock][acquired]: thread={}, lock_addr={}",
            tid,
            mutex as u64
        );
        ret
    } else {
        eprintln!("Failed to resolve pthread_mutex_lock");
        -1
    }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_mutex_unlock(mutex: *mut pthread_mutex_t) -> i32 {
    init();

    let tid: pthread_t = pthread_self();

    if let Some(real_pthread_mutex_unlock) = REAL_PTHREAD_MUTEX_UNLOCK {
        let ret = real_pthread_mutex_unlock(mutex);
        if ret == 0 {
            log::info!("[lock][unlock]: thread={}, lock_addr={}", tid, mutex as u64);
        }

        ret
    } else {
        eprintln!("Failed to resolve pthread_mutex_lock");
        -1
    }
}
