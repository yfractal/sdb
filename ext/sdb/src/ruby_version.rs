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
            let path_str = self.ruby_str_to_rust_str(path);

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
        unsafe fn is_iseq_imemo(&self, iseq_ptr: *const c_void) -> bool {
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
    Ruby315,
    Ruby322,
    Ruby330,
}

pub trait RubyApiCompat: Send + Sync {
    unsafe fn get_iseq_info(&self, iseq_addr: u64) -> (Option<String>, Option<String>);
    unsafe fn get_first_lineno(&self, iseq_addr: u64) -> VALUE;
    unsafe fn get_label(&self, iseq_addr: u64) -> VALUE;
    unsafe fn get_base_label(&self, iseq_addr: u64) -> VALUE;
    unsafe fn ruby_str_to_rust_str(&self, ruby_str: VALUE) -> Option<String>;
    unsafe fn is_iseq_imemo(&self, iseq_ptr: *const c_void) -> bool;
    unsafe fn get_ec_from_thread(&self, thread_val: VALUE) -> *mut c_void;
    fn get_control_frame_struct_size(&self) -> usize;
    unsafe fn iterate_frame_iseqs(&self, ec_val: VALUE, frame_handler: &mut dyn FnMut(u64));
}

pub struct Ruby315;

impl RubyApiCompat for Ruby315 {
    impl_iseq_functions!(rbspy_ruby_structs::ruby_3_1_5::rb_iseq_struct);
    impl_thread_functions!(rbspy_ruby_structs::ruby_3_1_5::rb_thread_t);
    impl_control_frame_functions!(
        rbspy_ruby_structs::ruby_3_1_5::rb_control_frame_struct,
        rbspy_ruby_structs::ruby_3_1_5::rb_execution_context_struct
    );
    impl_ruby_str_to_rust_str!(rbspy_ruby_structs::ruby_3_1_5::RString);
}

pub struct Ruby322;

impl RubyApiCompat for Ruby322 {
    impl_iseq_functions!(rbspy_ruby_structs::ruby_3_2_5::rb_iseq_struct);
    impl_thread_functions!(rbspy_ruby_structs::ruby_3_2_5::rb_thread_t);
    impl_control_frame_functions!(
        rbspy_ruby_structs::ruby_3_2_5::rb_control_frame_struct,
        rbspy_ruby_structs::ruby_3_2_5::rb_execution_context_struct
    );

    impl_ruby_str_to_rust_str!(rbspy_ruby_structs::ruby_3_2_5::RString);
}

pub struct Ruby330;

impl RubyApiCompat for Ruby330 {
    impl_iseq_functions!(rbspy_ruby_structs::ruby_3_3_1::rb_iseq_struct);
    impl_thread_functions!(rbspy_ruby_structs::ruby_3_3_1::rb_thread_t);
    impl_control_frame_functions!(
        rbspy_ruby_structs::ruby_3_3_1::rb_control_frame_struct,
        rbspy_ruby_structs::ruby_3_3_1::rb_execution_context_struct
    );

    impl_ruby_str_to_rust_str!(rbspy_ruby_structs::ruby_3_3_1::RString);
}

// Main API struct
pub struct RubyAPI {
    inner: Box<dyn RubyApiCompat>,
}

impl RubyAPI {
    pub fn new(version: RubyVersion) -> Self {
        let inner: Box<dyn RubyApiCompat> = match version {
            RubyVersion::Ruby315 => Box::new(Ruby315),
            RubyVersion::Ruby322 => Box::new(Ruby322),
            RubyVersion::Ruby330 => Box::new(Ruby330),
        };

        RubyAPI { inner }
    }

    pub unsafe fn get_iseq_info(&self, iseq_addr: u64) -> (Option<String>, Option<String>) {
        self.inner.get_iseq_info(iseq_addr)
    }

    pub unsafe fn get_first_lineno(&self, iseq_addr: u64) -> VALUE {
        self.inner.get_first_lineno(iseq_addr)
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

        if version_str.starts_with("3.1.5") {
            RubyVersion::Ruby315
        } else if version_str.starts_with("3.2.") {
            RubyVersion::Ruby322
        } else if version_str.starts_with("3.3.") {
            RubyVersion::Ruby330
        } else {
            log::warn!(
                "Unknown Ruby version: {}, falling back to 3.1.5 structs",
                version_str
            );
            RubyVersion::Ruby315
        }
    }
}
