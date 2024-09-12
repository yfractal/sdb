extern crate libc;
extern crate libloading;

use fast_log::config::Config;
use libc::{c_char, c_void, pthread_self, pthread_t};
use libc::{pthread_cond_t, pthread_mutex_t};
use libloading::Library;

use flate2::read::GzDecoder;
use std::io::{self, Read};
use std::slice;
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

static mut REAL_SSL_READ: Option<
    unsafe extern "C" fn(ssl: *const c_void, buf: *const c_char, num: i32) -> i32,
> = None;

unsafe fn init_once() {
    INIT.call_once(|| {
        fast_log::init(Config::new().file("sdb-lock.log").chan_len(Some(1_000_000))).unwrap();

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

        let openssl_file = std::env::var("OPENSSL_FILE")
            .expect("Please offer Ruby openssl filename through OPENSSL_FILE env");
        let open_ssl_lib = Library::new(openssl_file).expect("Failed to load openssl");

        let ssl_read_func: libloading::Symbol<
            unsafe extern "C" fn(ssl: *const c_void, buf: *const c_char, num: i32) -> i32,
        > = open_ssl_lib
            .get(b"SSL_read")
            .expect("Failed to load SSL_read symbol");
        REAL_SSL_READ = Some(*ssl_read_func);
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

fn decompress_zlib(data: &[u8]) -> io::Result<Vec<u8>> {
    let mut decoder = GzDecoder::new(data);
    let mut decompressed_data = Vec::new();

    decoder.read_to_end(&mut decompressed_data)?;

    Ok(decompressed_data)
}

fn split_headers_and_body(response: &[u8]) -> (&[u8], &[u8]) {
    let split_pos = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("Failed to find headers and body separator");

    let headers = &response[..split_pos];
    let body = &response[(split_pos + 4)..];

    (headers, body)
}

fn read_chunked_http_body(mut body: &[u8]) -> Vec<u8> {
    let mut decoded_body = Vec::new();

    loop {
        if body.len() == 0 {
            break;
        }

        let (chunk_size, rest) = read_chunk_size(body);
        body = rest;

        if chunk_size == 0 {
            break;
        }

        let (chunk_data, rest) = body.split_at(chunk_size);
        decoded_body.extend_from_slice(chunk_data);
        body = rest;

        body = skip_crlf(body);
    }

    decoded_body
}

fn read_chunk_size(body: &[u8]) -> (usize, &[u8]) {
    let pos = body
        .windows(2)
        .position(|window| window == b"\r\n")
        .expect("Invalid chunk size");

    let chunk_size_str = std::str::from_utf8(&body[..pos]).expect("Invalid UTF-8 in chunk size");

    let chunk_size = usize::from_str_radix(chunk_size_str.trim(), 16).expect("Invalid chunk size");

    (chunk_size, &body[pos + 2..])
}

fn skip_crlf(body: &[u8]) -> &[u8] {
    assert!(body.starts_with(b"\r\n"));
    &body[2..]
}

#[no_mangle]
pub unsafe extern "C" fn SSL_read(ssl: *const c_void, buf: *const c_char, len: i32) -> i32 {
    init_once();

    if let Some(real_ssl_read) = REAL_SSL_READ {
        let ret = real_ssl_read(ssl, buf, len);

        let byte_ptr = buf as *const u8;

        if ret > 0 {
            let slice = slice::from_raw_parts(byte_ptr, ret as usize);
            let (_, raw_body) = split_headers_and_body(slice);
            let body = read_chunked_http_body(raw_body);
            if body.len() != 0 {
                let decompressed_body = decompress_zlib(&body).unwrap();
                log::info!(
                    "[sdb-shim][ssl]: decoded_body={:?}",
                    std::str::from_utf8(&decompressed_body)
                );
            }
        }

        ret
    } else {
        eprintln!("Failed to resolve real_pthread_cond_wait");
        -1
    }
}
