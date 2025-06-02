use libc::{c_char, c_int, c_long, c_void};
use rb_sys::{rb_funcallv, rb_intern2, rb_num2long, Qnil, ID, VALUE};
use rbspy_ruby_structs::ruby_3_1_5::{rb_iseq_struct, RString};

const MAX_STR_LENGTH: usize = 127;
const RSTRING_HEAP_FLAGS: usize = 1 << 13;
const FL_USHIFT: usize = 12;
const IMEMO_MASK: usize = 0x0F;
const IMEMO_ISEQ: usize = 7;

#[inline]
pub(crate) fn internal_id(string: &str) -> ID {
    let str = string.as_ptr() as *const c_char;
    let len = string.len() as c_long;

    unsafe { rb_intern2(str, len) }
}

#[inline]
pub(crate) fn call_method(receiver: VALUE, method: &str, argc: c_int, argv: &[VALUE]) -> VALUE {
    let id = internal_id(method);
    unsafe { rb_funcallv(receiver, id, argc, argv as *const [VALUE] as *const VALUE) }
}

pub(crate) unsafe extern "C" fn rb_first_lineno_from_iseq_addr(
    _module: VALUE,
    iseq_addr: VALUE,
) -> VALUE {
    let iseq_addr = rb_num2long(iseq_addr) as *const c_void as u64;

    if iseq_addr == 0 {
        return Qnil as VALUE;
    }

    let iseq = &*(iseq_addr as *const rb_iseq_struct);
    let body = &*iseq.body;

    body.location.first_lineno as VALUE
}

pub(crate) unsafe extern "C" fn rb_label_from_iseq_addr(_module: VALUE, iseq_addr: VALUE) -> VALUE {
    let iseq_addr = rb_num2long(iseq_addr) as *const c_void as u64;

    if iseq_addr == 0 {
        return Qnil as VALUE;
    }

    let iseq = &*(iseq_addr as *const rb_iseq_struct);
    let body = &*iseq.body;

    body.location.label as VALUE
}

pub(crate) unsafe extern "C" fn rb_base_label_from_iseq_addr(
    _module: VALUE,
    iseq_addr: VALUE,
) -> VALUE {
    let iseq_addr = rb_num2long(iseq_addr) as *const c_void as u64;

    if iseq_addr == 0 {
        return Qnil as VALUE;
    }

    let iseq = &*(iseq_addr as *const rb_iseq_struct);
    let body = &*iseq.body;

    body.location.base_label as VALUE
}

#[inline]
pub(crate) unsafe fn ruby_str_to_rust_str(ruby_str: VALUE) -> Option<String> {
    let str_ptr = ruby_str as *const RString;
    if str_ptr.is_null() {
        return None;
    }

    let str_ref = &*str_ptr;
    let flags = str_ref.basic.flags;

    if flags & RSTRING_HEAP_FLAGS != 0 {
        // Heap string
        let len = (str_ref.as_.heap.aux.capa & 0x7F) as usize;
        let ptr = str_ref.as_.heap.ptr;

        if ptr.is_null() || len == 0 {
            None
        } else {
            // len - 1 for removing the last \0
            if *ptr.add(len - 1) == 0 {
                let bytes = std::slice::from_raw_parts(ptr as *const u8, len - 1);
                Some(String::from_utf8_lossy(bytes).into_owned())
            } else {
                let bytes = std::slice::from_raw_parts(ptr as *const u8, len);
                Some(String::from_utf8_lossy(bytes).into_owned())
            }
        }
    } else {
        // Embedded string
        let ary = str_ref.as_.embed.ary.as_ptr();
        let mut len = 0;

        for i in 0..MAX_STR_LENGTH {
            if *ary.add(i) == 0 {
                break;
            }
            len += 1;
        }

        let bytes = std::slice::from_raw_parts(ary as *const u8, len);
        Some(String::from_utf8_lossy(bytes).into_owned())
    }
}

// see `static inline enum imemo_type imemo_type(VALUE imemo)` func in Ruby
#[inline]
pub(crate) fn is_iseq_imemo(iseq: &rb_iseq_struct) -> bool {
    (iseq.flags >> FL_USHIFT) & IMEMO_MASK == IMEMO_ISEQ
}
