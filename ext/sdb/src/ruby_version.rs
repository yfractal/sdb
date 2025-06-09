use libc::c_void;
use rb_sys::VALUE;
use std::ffi::CStr;
use std::os::raw::c_char;

const MAX_STR_LENGTH: usize = 127;
const RSTRING_HEAP_FLAGS: usize = 1 << 13;

macro_rules! impl_ruby_str_to_rust_str {
    ($rstring_type:path) => {
        #[inline]
        unsafe fn ruby_str_to_rust_str(&self, ruby_str: VALUE) -> Option<String> {
            use $rstring_type as RString;

            let str_ptr = ruby_str as *const RString;

            if ruby_str == 0 {
                return None;
            }

            if str_ptr.is_null() {
                return None;
            }

            let str_ref = &*str_ptr;
            let flags = str_ref.basic.flags;

            if flags & RSTRING_HEAP_FLAGS != 0 {
                // For heap strings, we need to use strlen to find the end of the string
                // because the length is not readily available in rbspy structs
                let ptr = str_ref.as_.heap.ptr;

                if ptr.is_null() {
                    None
                } else {
                    // Use strlen to find the length of the null-terminated string
                    let len = rb_sys::RSTRING_LEN(ruby_str) as usize;

                    if len == 0 {
                        Some(String::new())
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
    };
}

macro_rules! impl_iseq_functions {
    ($iseq_struct:path) => {
        #[inline]
        unsafe fn get_iseq_info(&self, iseq_addr: u64) -> (Option<String>, Option<String>) {
            use $iseq_struct as rb_iseq_struct;
            let iseq = &*(iseq_addr as *const rb_iseq_struct);
            let body = &*iseq.body;

            let label = body.location.label as VALUE;
            let label_str = self.ruby_str_to_rust_str(label);

            let path = body.location.pathobj as VALUE;
            let path_str = self.extract_path_string(path);

            (label_str, path_str)
        }

        #[inline]
        unsafe fn get_first_lineno(&self, iseq_addr: u64) -> VALUE {
            use $iseq_struct as rb_iseq_struct;
            let iseq = &*(iseq_addr as *const rb_iseq_struct);
            let body = &*iseq.body;
            body.location.first_lineno as VALUE
        }

        #[inline]
        unsafe fn get_label(&self, iseq_addr: u64) -> VALUE {
            use $iseq_struct as rb_iseq_struct;
            let iseq = &*(iseq_addr as *const rb_iseq_struct);
            let body = &*iseq.body;
            body.location.label as VALUE
        }

        #[inline]
        unsafe fn get_base_label(&self, iseq_addr: u64) -> VALUE {
            use $iseq_struct as rb_iseq_struct;
            let iseq = &*(iseq_addr as *const rb_iseq_struct);
            let body = &*iseq.body;
            body.location.base_label as VALUE
        }

        #[inline]
        unsafe fn extract_path_string(&self, path: VALUE) -> Option<String> {
            if path == 0 {
                return None;
            }

            let basic_flags = *(path as *const rb_sys::RBasic);
            let obj_type = basic_flags.flags & (rb_sys::RUBY_T_MASK as u64);

            if obj_type == rb_sys::RUBY_T_STRING as u64 {
                self.ruby_str_to_rust_str(path)
            } else if obj_type == rb_sys::RUBY_T_ARRAY as u64 {
                let array_len = rb_sys::RARRAY_LEN(path);
                if array_len >= 1 {
                    let path_val = rb_sys::rb_ary_entry(path, 0);
                    self.ruby_str_to_rust_str(path_val)
                } else {
                    None
                }
            } else {
                None
            }
        }

        #[inline]
        unsafe fn is_iseq_imemo(&self, iseq_ptr: *const c_void) -> bool {
            if iseq_ptr.is_null() {
                return false;
            }

            use $iseq_struct as rb_iseq_struct;
            let iseq = &*(iseq_ptr as *const rb_iseq_struct);
            const FL_USHIFT: usize = 12;
            const IMEMO_MASK: usize = 0x0F;
            const IMEMO_ISEQ: usize = 7;
            (iseq.flags >> FL_USHIFT) & IMEMO_MASK == IMEMO_ISEQ
        }
    };
}

macro_rules! impl_thread_functions {
    ($thread_struct:path) => {
        #[inline]
        unsafe fn get_ec_from_thread(&self, thread_val: VALUE) -> *mut c_void {
            use rb_sys::RTypedData;
            use $thread_struct as rb_thread_t;
            let thread_ptr: *mut RTypedData = thread_val as *mut RTypedData;
            let thread_struct_ptr: *mut rb_thread_t = (*thread_ptr).data as *mut rb_thread_t;
            let thread_struct = &*thread_struct_ptr;
            thread_struct.ec as *mut c_void
        }
    };
}

macro_rules! impl_control_frame_functions {
    ($control_frame_struct:path, $execution_context_struct:path) => {
        #[inline]
        fn get_control_frame_struct_size(&self) -> usize {
            use $control_frame_struct as rb_control_frame_struct;
            std::mem::size_of::<rb_control_frame_struct>()
        }

        #[inline]
        unsafe fn iterate_frame_iseqs(&self, ec_val: VALUE, iseq_handler: &mut dyn FnMut(u64)) {
            use $execution_context_struct as rb_execution_context_struct;
            let ec = *(ec_val as *mut rb_execution_context_struct);
            let stack_base = ec.vm_stack.add(ec.vm_stack_size);
            let diff = (stack_base as usize) - (ec.cfp as usize);
            let len = diff / self.get_control_frame_struct_size();
            let frames = std::slice::from_raw_parts(ec.cfp, len);

            for frame in frames {
                let iseq = &*frame.iseq;
                let iseq_addr = iseq as *const _ as u64;
                iseq_handler(iseq_addr);
            }
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RubyVersion {
    // Ruby 3.1.x
    Ruby310,
    Ruby311,
    Ruby312,
    Ruby313,
    Ruby314,
    Ruby315,
    Ruby316,
    Ruby317,

    // Ruby 3.2.x
    Ruby320,
    Ruby321,
    Ruby322,
    Ruby323,
    Ruby324,
    Ruby325,
    Ruby326,
    Ruby327,
    Ruby328,

    // Ruby 3.3.x
    Ruby330,
    Ruby331,
    Ruby332,
    Ruby333,
    Ruby334,
    Ruby335,
    Ruby336,
    Ruby337,
    Ruby338,

    // Ruby 3.4.x
    Ruby340,
    Ruby341,
    Ruby342,
    Ruby343,
    Ruby344,
}

pub trait RubyApiCompat: Send + Sync {
    unsafe fn get_iseq_info(&self, iseq_addr: u64) -> (Option<String>, Option<String>);
    unsafe fn get_first_lineno(&self, iseq_addr: u64) -> VALUE;
    unsafe fn get_label(&self, iseq_addr: u64) -> VALUE;
    unsafe fn get_base_label(&self, iseq_addr: u64) -> VALUE;
    unsafe fn ruby_str_to_rust_str(&self, ruby_str: VALUE) -> Option<String>;
    unsafe fn extract_path_string(&self, path: VALUE) -> Option<String>;
    unsafe fn is_iseq_imemo(&self, iseq_ptr: *const c_void) -> bool;
    unsafe fn get_ec_from_thread(&self, thread_val: VALUE) -> *mut c_void;
    fn get_control_frame_struct_size(&self) -> usize;
    unsafe fn iterate_frame_iseqs(&self, ec_val: VALUE, frame_handler: &mut dyn FnMut(u64));
}

// Macro to reduce duplication for Ruby version implementations
macro_rules! impl_ruby_version {
    // Ruby 3.1.x
    (Ruby310) => {
        impl_ruby_version_with_module!(Ruby310, ruby_3_1_0);
    };
    (Ruby311) => {
        impl_ruby_version_with_module!(Ruby311, ruby_3_1_1);
    };
    (Ruby312) => {
        impl_ruby_version_with_module!(Ruby312, ruby_3_1_2);
    };
    (Ruby313) => {
        impl_ruby_version_with_module!(Ruby313, ruby_3_1_3);
    };
    (Ruby314) => {
        impl_ruby_version_with_module!(Ruby314, ruby_3_1_4);
    };
    (Ruby315) => {
        impl_ruby_version_with_module!(Ruby315, ruby_3_1_5);
    };
    (Ruby316) => {
        impl_ruby_version_with_module!(Ruby316, ruby_3_1_6);
    };
    (Ruby317) => {
        impl_ruby_version_with_module!(Ruby317, ruby_3_1_7);
    };

    // Ruby 3.2.x
    (Ruby320) => {
        impl_ruby_version_with_module!(Ruby320, ruby_3_2_0);
    };
    (Ruby321) => {
        impl_ruby_version_with_module!(Ruby321, ruby_3_2_1);
    };
    (Ruby322) => {
        impl_ruby_version_with_module!(Ruby322, ruby_3_2_2);
    };
    (Ruby323) => {
        impl_ruby_version_with_module!(Ruby323, ruby_3_2_3);
    };
    (Ruby324) => {
        impl_ruby_version_with_module!(Ruby324, ruby_3_2_4);
    };
    (Ruby325) => {
        impl_ruby_version_with_module!(Ruby325, ruby_3_2_5);
    };
    (Ruby326) => {
        impl_ruby_version_with_module!(Ruby326, ruby_3_2_6);
    };
    (Ruby327) => {
        impl_ruby_version_with_module!(Ruby327, ruby_3_2_7);
    };
    (Ruby328) => {
        impl_ruby_version_with_module!(Ruby328, ruby_3_2_8);
    };

    // Ruby 3.3.x
    (Ruby330) => {
        impl_ruby_version_with_module!(Ruby330, ruby_3_3_0);
    };
    (Ruby331) => {
        impl_ruby_version_with_module!(Ruby331, ruby_3_3_1);
    };
    (Ruby332) => {
        impl_ruby_version_with_module!(Ruby332, ruby_3_3_2);
    };
    (Ruby333) => {
        impl_ruby_version_with_module!(Ruby333, ruby_3_3_3);
    };
    (Ruby334) => {
        impl_ruby_version_with_module!(Ruby334, ruby_3_3_4);
    };
    (Ruby335) => {
        impl_ruby_version_with_module!(Ruby335, ruby_3_3_5);
    };
    (Ruby336) => {
        impl_ruby_version_with_module!(Ruby336, ruby_3_3_6);
    };
    (Ruby337) => {
        impl_ruby_version_with_module!(Ruby337, ruby_3_3_7);
    };
    (Ruby338) => {
        impl_ruby_version_with_module!(Ruby338, ruby_3_3_8);
    };

    // Ruby 3.4.x
    (Ruby340) => {
        impl_ruby_version_with_module!(Ruby340, ruby_3_4_0);
    };
    (Ruby341) => {
        impl_ruby_version_with_module!(Ruby341, ruby_3_4_1);
    };
    (Ruby342) => {
        impl_ruby_version_with_module!(Ruby342, ruby_3_4_2);
    };
    (Ruby343) => {
        impl_ruby_version_with_module!(Ruby343, ruby_3_4_3);
    };
    (Ruby344) => {
        impl_ruby_version_with_module!(Ruby344, ruby_3_4_4);
    };
}

// Helper macro that does the actual implementation
macro_rules! impl_ruby_version_with_module {
    ($struct_name:ident, $module:ident) => {
        pub struct $struct_name;

        impl RubyApiCompat for $struct_name {
            impl_iseq_functions!(rbspy_ruby_structs::$module::rb_iseq_struct);
            impl_thread_functions!(rbspy_ruby_structs::$module::rb_thread_t);
            impl_control_frame_functions!(
                rbspy_ruby_structs::$module::rb_control_frame_struct,
                rbspy_ruby_structs::$module::rb_execution_context_struct
            );
            impl_ruby_str_to_rust_str!(rbspy_ruby_structs::$module::RString);
        }
    };
}

// Ruby 3.1.x implementations
impl_ruby_version!(Ruby310);
impl_ruby_version!(Ruby311);
impl_ruby_version!(Ruby312);
impl_ruby_version!(Ruby313);
impl_ruby_version!(Ruby314);
impl_ruby_version!(Ruby315);
impl_ruby_version!(Ruby316);
impl_ruby_version!(Ruby317);

// Ruby 3.2.x implementations
impl_ruby_version!(Ruby320);
impl_ruby_version!(Ruby321);
impl_ruby_version!(Ruby322);
impl_ruby_version!(Ruby323);
impl_ruby_version!(Ruby324);
impl_ruby_version!(Ruby325);
impl_ruby_version!(Ruby326);
impl_ruby_version!(Ruby327);
impl_ruby_version!(Ruby328);

// Ruby 3.3.x implementations
impl_ruby_version!(Ruby330);
impl_ruby_version!(Ruby331);
impl_ruby_version!(Ruby332);
impl_ruby_version!(Ruby333);
impl_ruby_version!(Ruby334);
impl_ruby_version!(Ruby335);
impl_ruby_version!(Ruby336);
impl_ruby_version!(Ruby337);
impl_ruby_version!(Ruby338);

// Ruby 3.4.x implementations
impl_ruby_version!(Ruby340);
impl_ruby_version!(Ruby341);
impl_ruby_version!(Ruby342);
impl_ruby_version!(Ruby343);
impl_ruby_version!(Ruby344);

// Main API struct
pub struct RubyAPI {
    inner: Box<dyn RubyApiCompat>,
}

impl RubyAPI {
    pub fn new(version: RubyVersion) -> Self {
        let inner: Box<dyn RubyApiCompat> = match version {
            // Ruby 3.1.x
            RubyVersion::Ruby310 => Box::new(Ruby310),
            RubyVersion::Ruby311 => Box::new(Ruby311),
            RubyVersion::Ruby312 => Box::new(Ruby312),
            RubyVersion::Ruby313 => Box::new(Ruby313),
            RubyVersion::Ruby314 => Box::new(Ruby314),
            RubyVersion::Ruby315 => Box::new(Ruby315),
            RubyVersion::Ruby316 => Box::new(Ruby316),
            RubyVersion::Ruby317 => Box::new(Ruby317),

            // Ruby 3.2.x
            RubyVersion::Ruby320 => Box::new(Ruby320),
            RubyVersion::Ruby321 => Box::new(Ruby321),
            RubyVersion::Ruby322 => Box::new(Ruby322),
            RubyVersion::Ruby323 => Box::new(Ruby323),
            RubyVersion::Ruby324 => Box::new(Ruby324),
            RubyVersion::Ruby325 => Box::new(Ruby325),
            RubyVersion::Ruby326 => Box::new(Ruby326),
            RubyVersion::Ruby327 => Box::new(Ruby327),
            RubyVersion::Ruby328 => Box::new(Ruby328),

            // Ruby 3.3.x
            RubyVersion::Ruby330 => Box::new(Ruby330),
            RubyVersion::Ruby331 => Box::new(Ruby331),
            RubyVersion::Ruby332 => Box::new(Ruby332),
            RubyVersion::Ruby333 => Box::new(Ruby333),
            RubyVersion::Ruby334 => Box::new(Ruby334),
            RubyVersion::Ruby335 => Box::new(Ruby335),
            RubyVersion::Ruby336 => Box::new(Ruby336),
            RubyVersion::Ruby337 => Box::new(Ruby337),
            RubyVersion::Ruby338 => Box::new(Ruby338),

            // Ruby 3.4.x
            RubyVersion::Ruby340 => Box::new(Ruby340),
            RubyVersion::Ruby341 => Box::new(Ruby341),
            RubyVersion::Ruby342 => Box::new(Ruby342),
            RubyVersion::Ruby343 => Box::new(Ruby343),
            RubyVersion::Ruby344 => Box::new(Ruby344),
        };

        RubyAPI { inner }
    }

    pub unsafe fn get_iseq_info(&self, iseq_addr: u64) -> (Option<String>, Option<String>) {
        self.inner.get_iseq_info(iseq_addr)
    }

    pub unsafe fn get_first_lineno(&self, iseq_addr: u64) -> VALUE {
        self.inner.get_first_lineno(iseq_addr)
    }

    pub unsafe fn ruby_str_to_rust_str(&self, ruby_str: VALUE) -> Option<String> {
        self.inner.ruby_str_to_rust_str(ruby_str)
    }

    pub unsafe fn get_label(&self, iseq_addr: u64) -> VALUE {
        self.inner.get_label(iseq_addr)
    }

    pub unsafe fn get_base_label(&self, iseq_addr: u64) -> VALUE {
        self.inner.get_base_label(iseq_addr)
    }

    pub unsafe fn is_iseq_imemo(&self, iseq_ptr: *const c_void) -> bool {
        self.inner.is_iseq_imemo(iseq_ptr)
    }

    pub unsafe fn get_ec_from_thread(&self, thread_val: VALUE) -> *mut c_void {
        self.inner.get_ec_from_thread(thread_val)
    }

    #[inline]
    pub unsafe fn iterate_frame_iseqs(&self, ec_val: VALUE, frame_handler: &mut dyn FnMut(u64)) {
        self.inner.iterate_frame_iseqs(ec_val, frame_handler)
    }
}

unsafe fn get_ruby_version_string() -> String {
    let version_sym = rb_sys::rb_intern("RUBY_VERSION\0".as_ptr() as *const c_char);
    let version_val = rb_sys::rb_const_get(rb_sys::rb_cObject, version_sym);

    let version_ptr = rb_sys::rb_string_value_cstr(&version_val as *const _ as *mut _);
    let version_cstr = CStr::from_ptr(version_ptr);
    version_cstr.to_string_lossy().to_string()
}

pub fn detect_ruby_version() -> RubyVersion {
    unsafe {
        let version_str = get_ruby_version_string();

        // Ruby 3.1.x
        if version_str.starts_with("3.1.0") {
            RubyVersion::Ruby310
        } else if version_str.starts_with("3.1.1") {
            RubyVersion::Ruby311
        } else if version_str.starts_with("3.1.2") {
            RubyVersion::Ruby312
        } else if version_str.starts_with("3.1.3") {
            RubyVersion::Ruby313
        } else if version_str.starts_with("3.1.4") {
            RubyVersion::Ruby314
        } else if version_str.starts_with("3.1.5") {
            RubyVersion::Ruby315
        } else if version_str.starts_with("3.1.6") {
            RubyVersion::Ruby316
        } else if version_str.starts_with("3.1.7") {
            RubyVersion::Ruby317

        // Ruby 3.2.x
        } else if version_str.starts_with("3.2.0") {
            RubyVersion::Ruby320
        } else if version_str.starts_with("3.2.1") {
            RubyVersion::Ruby321
        } else if version_str.starts_with("3.2.2") {
            RubyVersion::Ruby322
        } else if version_str.starts_with("3.2.3") {
            RubyVersion::Ruby323
        } else if version_str.starts_with("3.2.4") {
            RubyVersion::Ruby324
        } else if version_str.starts_with("3.2.5") {
            RubyVersion::Ruby325
        } else if version_str.starts_with("3.2.6") {
            RubyVersion::Ruby326
        } else if version_str.starts_with("3.2.7") {
            RubyVersion::Ruby327
        } else if version_str.starts_with("3.2.8") {
            RubyVersion::Ruby328

        // Ruby 3.3.x
        } else if version_str.starts_with("3.3.0") {
            RubyVersion::Ruby330
        } else if version_str.starts_with("3.3.1") {
            RubyVersion::Ruby331
        } else if version_str.starts_with("3.3.2") {
            RubyVersion::Ruby332
        } else if version_str.starts_with("3.3.3") {
            RubyVersion::Ruby333
        } else if version_str.starts_with("3.3.4") {
            RubyVersion::Ruby334
        } else if version_str.starts_with("3.3.5") {
            RubyVersion::Ruby335
        } else if version_str.starts_with("3.3.6") {
            RubyVersion::Ruby336
        } else if version_str.starts_with("3.3.7") {
            RubyVersion::Ruby337
        } else if version_str.starts_with("3.3.8") {
            RubyVersion::Ruby338

        // Ruby 3.4.x
        } else if version_str.starts_with("3.4.0") {
            RubyVersion::Ruby340
        } else if version_str.starts_with("3.4.1") {
            RubyVersion::Ruby341
        } else if version_str.starts_with("3.4.2") {
            RubyVersion::Ruby342
        } else if version_str.starts_with("3.4.3") {
            RubyVersion::Ruby343
        } else if version_str.starts_with("3.4.4") {
            RubyVersion::Ruby344
        } else {
            panic!("Unknown Ruby version: {}", version_str);
        }
    }
}
