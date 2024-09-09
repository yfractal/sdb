extern crate libc;
extern crate libloading;

use fast_log::config::Config;
use libc::{pthread_cond_t, pthread_mutex_t};
use libc::{pthread_self, pthread_t};
use libloading::Library;
use std::sync::Once;

static INIT: Once = Once::new();
static mut REAL_PTHREAD_MUTEX_LOCK: Option<unsafe extern "C" fn(*mut pthread_mutex_t) -> i32> =
    None;
static mut REAL_PTHREAD_MUTEX_UNLOCK: Option<unsafe extern "C" fn(*mut pthread_mutex_t) -> i32> =
    None;

static mut REAL_PTHREAD_COND_WAIT: Option<
    unsafe extern "C" fn(cond: *mut pthread_cond_t, *mut pthread_mutex_t) -> i32,
> = None;

static mut REAL_PTHREAD_COND_SIGNAL: Option<
    unsafe extern "C" fn(cond: *mut pthread_cond_t) -> i32,
> = None;

unsafe fn init_once() {
    INIT.call_once(|| {
        fast_log::init(Config::new().file("lock.log").chan_len(Some(1_000_000))).unwrap();

        let lib = Library::new("libpthread.so.0").expect("Failed to load libpthread");

        let lock_func: libloading::Symbol<unsafe extern "C" fn(*mut pthread_mutex_t) -> i32> = lib
            .get(b"pthread_mutex_lock")
            .expect("Failed to load pthread_mutex_lock symbol");
        REAL_PTHREAD_MUTEX_LOCK = Some(*lock_func);

        let unlock_func: libloading::Symbol<unsafe extern "C" fn(*mut pthread_mutex_t) -> i32> =
            lib.get(b"pthread_mutex_unlock")
                .expect("Failed to load pthread_mutex_unlock symbol");
        REAL_PTHREAD_MUTEX_UNLOCK = Some(*unlock_func);

        let cond_wait_func: libloading::Symbol<
            unsafe extern "C" fn(cond: *mut pthread_cond_t, *mut pthread_mutex_t) -> i32,
        > = lib
            .get(b"pthread_cond_wait")
            .expect("Failed to load pthread_cond_wait symbol");
        REAL_PTHREAD_COND_WAIT = Some(*cond_wait_func);

        let cond_signal_func: libloading::Symbol<
            unsafe extern "C" fn(cond: *mut pthread_cond_t) -> i32,
        > = lib
            .get(b"pthread_cond_signal")
            .expect("Failed to load pthread_cond_signal symbol");
        REAL_PTHREAD_COND_SIGNAL = Some(*cond_signal_func);
    });
}

#[no_mangle]
pub unsafe extern "C" fn pthread_mutex_lock(mutex: *mut pthread_mutex_t) -> i32 {
    init_once();

    let tid: pthread_t = pthread_self();
    log::info!(
        "[lock][mutex][acquire]: thread={}, lock_addr={}",
        tid,
        mutex as u64
    );

    if let Some(real_pthread_mutex_lock) = REAL_PTHREAD_MUTEX_LOCK {
        let ret = real_pthread_mutex_lock(mutex);
        log::info!(
            "[lock][mutex][acquired]: thread={}, lock_addr={}",
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
    init_once();

    let tid: pthread_t = pthread_self();

    if let Some(real_pthread_mutex_unlock) = REAL_PTHREAD_MUTEX_UNLOCK {
        let ret = real_pthread_mutex_unlock(mutex);
        if ret == 0 {
            log::info!(
                "[lock][mutex][unlock]: thread={}, lock_addr={}",
                tid,
                mutex as u64
            );
        }

        ret
    } else {
        eprintln!("Failed to resolve pthread_mutex_unlock");
        -1
    }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_cond_wait(
    cond: *mut pthread_cond_t,
    mutex: *mut pthread_mutex_t,
) -> i32 {
    init_once();

    let tid: pthread_t = pthread_self();
    log::info!(
        "[lock][cond][acquire]: thread={}, lock_addr={}, cond_var_addr={}",
        tid,
        mutex as u64,
        cond as u64
    );

    if let Some(real_pthread_cond_wait) = REAL_PTHREAD_COND_WAIT {
        let ret = real_pthread_cond_wait(cond, mutex);
        if ret == 0 {
            log::info!(
                "[lock][cond][acquired]: thread={}, lock_addr={}, cond_var_addr={}",
                tid,
                mutex as u64,
                cond as u64
            );
        }

        ret
    } else {
        eprintln!("Failed to resolve real_pthread_cond_wait");
        -1
    }
}

#[no_mangle]
pub unsafe extern "C" fn pthread_cond_signal(cond: *mut pthread_cond_t) -> i32 {
    init_once();

    let tid: pthread_t = pthread_self();

    if let Some(real_pthread_cond_signal) = REAL_PTHREAD_COND_SIGNAL {
        let ret = real_pthread_cond_signal(cond);
        if ret == 0 {
            log::info!(
                "[lock][cond][signal]: thread={}, cond_var_addr={}",
                tid,
                cond as u64
            );
        }

        ret
    } else {
        eprintln!("Failed to resolve real_pthread_cond_wait");
        -1
    }
}
