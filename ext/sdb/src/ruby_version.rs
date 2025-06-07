use libc::c_void;
use rb_sys::VALUE;
use std::ffi::CStr;
use std::os::raw::c_char;

// Supported Ruby versions
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RubyVersion {
    Ruby315,
    Ruby322,
    Ruby330,
    Unknown,
}

// Trait defining Ruby version-specific operations
pub trait RubyApiCompat: Send + Sync {
    unsafe fn get_iseq_info(&self, iseq_addr: u64) -> (Option<String>, Option<String>);
    unsafe fn get_first_lineno(&self, iseq_addr: u64) -> VALUE;
    unsafe fn get_label(&self, iseq_addr: u64) -> VALUE;
    unsafe fn get_base_label(&self, iseq_addr: u64) -> VALUE;
    unsafe fn ruby_str_to_rust_str(&self, ruby_str: VALUE) -> Option<String>;
    unsafe fn is_iseq_imemo(&self, iseq_ptr: *const c_void) -> bool;
    unsafe fn get_ec_from_thread(&self, thread_val: VALUE) -> *mut c_void;
    fn get_control_frame_struct_size(&self) -> usize;
    unsafe fn get_control_frame_slice(&self, ec_val: VALUE, len: usize) -> *const c_void;
    unsafe fn record_thread_frames(
        &self,
        thread_val: VALUE,
        ec_val: VALUE,
        trace_id: u64,
        timestamp: u64,
        iseq_logger: &mut crate::iseq_logger::IseqLogger,
        iseq_buffer: &mut std::collections::HashSet<u64>,
    );
}

// Ruby 3.1.5 implementation
pub struct Ruby315;

impl RubyApiCompat for Ruby315 {
    unsafe fn get_iseq_info(&self, iseq_addr: u64) -> (Option<String>, Option<String>) {
        use rbspy_ruby_structs::ruby_3_1_5::{rb_iseq_struct, RString};
        let iseq = &*(iseq_addr as *const rb_iseq_struct);
        let body = &*iseq.body;

        let label = body.location.label as VALUE;
        let label_str = self.ruby_str_to_rust_str(label);

        let path = body.location.pathobj as VALUE;
        let path_str = self.ruby_str_to_rust_str(path);

        (label_str, path_str)
    }

    unsafe fn get_first_lineno(&self, iseq_addr: u64) -> VALUE {
        use rbspy_ruby_structs::ruby_3_1_5::rb_iseq_struct;
        let iseq = &*(iseq_addr as *const rb_iseq_struct);
        let body = &*iseq.body;
        body.location.first_lineno as VALUE
    }

    unsafe fn get_label(&self, iseq_addr: u64) -> VALUE {
        use rbspy_ruby_structs::ruby_3_1_5::rb_iseq_struct;
        let iseq = &*(iseq_addr as *const rb_iseq_struct);
        let body = &*iseq.body;
        body.location.label as VALUE
    }

    unsafe fn get_base_label(&self, iseq_addr: u64) -> VALUE {
        use rbspy_ruby_structs::ruby_3_1_5::rb_iseq_struct;
        let iseq = &*(iseq_addr as *const rb_iseq_struct);
        let body = &*iseq.body;
        body.location.base_label as VALUE
    }

    unsafe fn ruby_str_to_rust_str(&self, ruby_str: VALUE) -> Option<String> {
        // TODO: Implement proper Ruby string conversion for 3.1.5
        None
    }

    unsafe fn is_iseq_imemo(&self, iseq_ptr: *const c_void) -> bool {
        use rbspy_ruby_structs::ruby_3_1_5::rb_iseq_struct;
        let iseq = &*(iseq_ptr as *const rb_iseq_struct);
        const FL_USHIFT: usize = 12;
        const IMEMO_MASK: usize = 0x0F;
        const IMEMO_ISEQ: usize = 7;
        (iseq.flags >> FL_USHIFT) & IMEMO_MASK == IMEMO_ISEQ
    }

    unsafe fn get_ec_from_thread(&self, thread_val: VALUE) -> *mut c_void {
        use rb_sys::RTypedData;
        use rbspy_ruby_structs::ruby_3_1_5::rb_thread_t;
        let thread_ptr: *mut RTypedData = thread_val as *mut RTypedData;
        let thread_struct_ptr: *mut rb_thread_t = (*thread_ptr).data as *mut rb_thread_t;
        let thread_struct = &*thread_struct_ptr;
        thread_struct.ec as *mut c_void
    }

    fn get_control_frame_struct_size(&self) -> usize {
        use rbspy_ruby_structs::ruby_3_1_5::rb_control_frame_struct;
        std::mem::size_of::<rb_control_frame_struct>()
    }

    unsafe fn get_control_frame_slice(&self, ec_val: VALUE, len: usize) -> *const c_void {
        use rbspy_ruby_structs::ruby_3_1_5::{
            rb_control_frame_struct, rb_execution_context_struct,
        };
        let ec = *(ec_val as *mut rb_execution_context_struct);
        ec.cfp as *const c_void
    }

    unsafe fn record_thread_frames(
        &self,
        thread_val: VALUE,
        ec_val: VALUE,
        trace_id: u64,
        timestamp: u64,
        iseq_logger: &mut crate::iseq_logger::IseqLogger,
        iseq_buffer: &mut std::collections::HashSet<u64>,
    ) {
        use rbspy_ruby_structs::ruby_3_1_5::{
            rb_control_frame_struct, rb_execution_context_struct,
        };
        let ec = *(ec_val as *mut rb_execution_context_struct);
        let stack_base = ec.vm_stack.add(ec.vm_stack_size);
        let diff = (stack_base as usize) - (ec.cfp as usize);
        let len = diff / self.get_control_frame_struct_size();
        let frames = std::slice::from_raw_parts(ec.cfp, len);

        iseq_logger.push(trace_id);
        iseq_logger.push(timestamp);

        for frame in frames {
            let iseq = &*frame.iseq;
            let iseq_addr = iseq as *const _ as u64;

            if iseq_addr == 0 {
                // C frame - skip for now
            } else {
                iseq_buffer.insert(iseq_addr);
                iseq_logger.push(iseq_addr);
            }
        }

        iseq_logger.push_seperator();
    }
}

// Ruby 3.2.x implementation
pub struct Ruby322;

impl RubyApiCompat for Ruby322 {
    unsafe fn get_iseq_info(&self, iseq_addr: u64) -> (Option<String>, Option<String>) {
        use rbspy_ruby_structs::ruby_3_2_5::{rb_iseq_struct, RString};
        let iseq = &*(iseq_addr as *const rb_iseq_struct);
        let body = &*iseq.body;

        let label = body.location.label as VALUE;
        let label_str = self.ruby_str_to_rust_str(label);

        let path = body.location.pathobj as VALUE;
        let path_str = self.ruby_str_to_rust_str(path);

        (label_str, path_str)
    }

    unsafe fn get_first_lineno(&self, iseq_addr: u64) -> VALUE {
        use rbspy_ruby_structs::ruby_3_2_5::rb_iseq_struct;
        let iseq = &*(iseq_addr as *const rb_iseq_struct);
        let body = &*iseq.body;
        body.location.first_lineno as VALUE
    }

    unsafe fn get_label(&self, iseq_addr: u64) -> VALUE {
        use rbspy_ruby_structs::ruby_3_2_5::rb_iseq_struct;
        let iseq = &*(iseq_addr as *const rb_iseq_struct);
        let body = &*iseq.body;
        body.location.label as VALUE
    }

    unsafe fn get_base_label(&self, iseq_addr: u64) -> VALUE {
        use rbspy_ruby_structs::ruby_3_2_5::rb_iseq_struct;
        let iseq = &*(iseq_addr as *const rb_iseq_struct);
        let body = &*iseq.body;
        body.location.base_label as VALUE
    }

    unsafe fn ruby_str_to_rust_str(&self, ruby_str: VALUE) -> Option<String> {
        // TODO: Implement proper Ruby string conversion for 3.2.x
        None
    }

    unsafe fn is_iseq_imemo(&self, iseq_ptr: *const c_void) -> bool {
        use rbspy_ruby_structs::ruby_3_2_5::rb_iseq_struct;
        let iseq = &*(iseq_ptr as *const rb_iseq_struct);
        const FL_USHIFT: usize = 12;
        const IMEMO_MASK: usize = 0x0F;
        const IMEMO_ISEQ: usize = 7;
        (iseq.flags >> FL_USHIFT) & IMEMO_MASK == IMEMO_ISEQ
    }

    unsafe fn get_ec_from_thread(&self, thread_val: VALUE) -> *mut c_void {
        use rb_sys::RTypedData;
        use rbspy_ruby_structs::ruby_3_2_5::rb_thread_t;
        let thread_ptr: *mut RTypedData = thread_val as *mut RTypedData;
        let thread_struct_ptr: *mut rb_thread_t = (*thread_ptr).data as *mut rb_thread_t;
        let thread_struct = &*thread_struct_ptr;
        thread_struct.ec as *mut c_void
    }

    fn get_control_frame_struct_size(&self) -> usize {
        use rbspy_ruby_structs::ruby_3_2_5::rb_control_frame_struct;
        std::mem::size_of::<rb_control_frame_struct>()
    }

    unsafe fn get_control_frame_slice(&self, ec_val: VALUE, len: usize) -> *const c_void {
        use rbspy_ruby_structs::ruby_3_2_5::{
            rb_control_frame_struct, rb_execution_context_struct,
        };
        let ec = *(ec_val as *mut rb_execution_context_struct);
        ec.cfp as *const c_void
    }

    unsafe fn record_thread_frames(
        &self,
        thread_val: VALUE,
        ec_val: VALUE,
        trace_id: u64,
        timestamp: u64,
        iseq_logger: &mut crate::iseq_logger::IseqLogger,
        iseq_buffer: &mut std::collections::HashSet<u64>,
    ) {
        use rbspy_ruby_structs::ruby_3_2_5::{
            rb_control_frame_struct, rb_execution_context_struct,
        };
        let ec = *(ec_val as *mut rb_execution_context_struct);
        let stack_base = ec.vm_stack.add(ec.vm_stack_size);
        let diff = (stack_base as usize) - (ec.cfp as usize);
        let len = diff / self.get_control_frame_struct_size();
        let frames = std::slice::from_raw_parts(ec.cfp, len);

        iseq_logger.push(trace_id);
        iseq_logger.push(timestamp);

        for frame in frames {
            let iseq = &*frame.iseq;
            let iseq_addr = iseq as *const _ as u64;

            if iseq_addr == 0 {
                // C frame - skip for now
            } else {
                iseq_buffer.insert(iseq_addr);
                iseq_logger.push(iseq_addr);
            }
        }

        iseq_logger.push_seperator();
    }
}

// Ruby 3.3.x implementation
pub struct Ruby330;

impl RubyApiCompat for Ruby330 {
    unsafe fn get_iseq_info(&self, iseq_addr: u64) -> (Option<String>, Option<String>) {
        use rbspy_ruby_structs::ruby_3_3_1::{rb_iseq_struct, RString};
        let iseq = &*(iseq_addr as *const rb_iseq_struct);
        let body = &*iseq.body;

        let label = body.location.label as VALUE;
        let label_str = self.ruby_str_to_rust_str(label);

        let path = body.location.pathobj as VALUE;
        let path_str = self.ruby_str_to_rust_str(path);

        (label_str, path_str)
    }

    unsafe fn get_first_lineno(&self, iseq_addr: u64) -> VALUE {
        use rbspy_ruby_structs::ruby_3_3_1::rb_iseq_struct;
        let iseq = &*(iseq_addr as *const rb_iseq_struct);
        let body = &*iseq.body;
        body.location.first_lineno as VALUE
    }

    unsafe fn get_label(&self, iseq_addr: u64) -> VALUE {
        use rbspy_ruby_structs::ruby_3_3_1::rb_iseq_struct;
        let iseq = &*(iseq_addr as *const rb_iseq_struct);
        let body = &*iseq.body;
        body.location.label as VALUE
    }

    unsafe fn get_base_label(&self, iseq_addr: u64) -> VALUE {
        use rbspy_ruby_structs::ruby_3_3_1::rb_iseq_struct;
        let iseq = &*(iseq_addr as *const rb_iseq_struct);
        let body = &*iseq.body;
        body.location.base_label as VALUE
    }

    unsafe fn ruby_str_to_rust_str(&self, ruby_str: VALUE) -> Option<String> {
        // TODO: Implement proper Ruby string conversion for 3.3.x
        None
    }

    unsafe fn is_iseq_imemo(&self, iseq_ptr: *const c_void) -> bool {
        use rbspy_ruby_structs::ruby_3_3_1::rb_iseq_struct;
        let iseq = &*(iseq_ptr as *const rb_iseq_struct);
        const FL_USHIFT: usize = 12;
        const IMEMO_MASK: usize = 0x0F;
        const IMEMO_ISEQ: usize = 7;
        (iseq.flags >> FL_USHIFT) & IMEMO_MASK == IMEMO_ISEQ
    }

    unsafe fn get_ec_from_thread(&self, thread_val: VALUE) -> *mut c_void {
        use rb_sys::RTypedData;
        use rbspy_ruby_structs::ruby_3_3_1::rb_thread_t;
        let thread_ptr: *mut RTypedData = thread_val as *mut RTypedData;
        let thread_struct_ptr: *mut rb_thread_t = (*thread_ptr).data as *mut rb_thread_t;
        let thread_struct = &*thread_struct_ptr;
        thread_struct.ec as *mut c_void
    }

    fn get_control_frame_struct_size(&self) -> usize {
        use rbspy_ruby_structs::ruby_3_3_1::rb_control_frame_struct;
        std::mem::size_of::<rb_control_frame_struct>()
    }

    unsafe fn get_control_frame_slice(&self, ec_val: VALUE, len: usize) -> *const c_void {
        use rbspy_ruby_structs::ruby_3_3_1::{
            rb_control_frame_struct, rb_execution_context_struct,
        };
        let ec = *(ec_val as *mut rb_execution_context_struct);
        ec.cfp as *const c_void
    }

    unsafe fn record_thread_frames(
        &self,
        thread_val: VALUE,
        ec_val: VALUE,
        trace_id: u64,
        timestamp: u64,
        iseq_logger: &mut crate::iseq_logger::IseqLogger,
        iseq_buffer: &mut std::collections::HashSet<u64>,
    ) {
        use rbspy_ruby_structs::ruby_3_3_1::{
            rb_control_frame_struct, rb_execution_context_struct,
        };
        let ec = *(ec_val as *mut rb_execution_context_struct);
        let stack_base = ec.vm_stack.add(ec.vm_stack_size);
        let diff = (stack_base as usize) - (ec.cfp as usize);
        let len = diff / self.get_control_frame_struct_size();
        let frames = std::slice::from_raw_parts(ec.cfp, len);

        iseq_logger.push(trace_id);
        iseq_logger.push(timestamp);

        for frame in frames {
            let iseq = &*frame.iseq;
            let iseq_addr = iseq as *const _ as u64;

            if iseq_addr == 0 {
                // C frame - skip for now
            } else {
                iseq_buffer.insert(iseq_addr);
                iseq_logger.push(iseq_addr);
            }
        }

        iseq_logger.push_seperator();
    }
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
            RubyVersion::Unknown => Box::new(Ruby315), // Fallback
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

    pub unsafe fn ruby_str_to_rust_str(&self, ruby_str: VALUE) -> Option<String> {
        self.inner.ruby_str_to_rust_str(ruby_str)
    }

    pub unsafe fn is_iseq_imemo(&self, iseq_ptr: *const c_void) -> bool {
        self.inner.is_iseq_imemo(iseq_ptr)
    }

    pub unsafe fn get_ec_from_thread(&self, thread_val: VALUE) -> *mut c_void {
        self.inner.get_ec_from_thread(thread_val)
    }

    pub fn get_control_frame_struct_size(&self) -> usize {
        self.inner.get_control_frame_struct_size()
    }

    pub unsafe fn get_control_frame_slice(&self, ec_val: VALUE, len: usize) -> *const c_void {
        self.inner.get_control_frame_slice(ec_val, len)
    }

    pub unsafe fn record_thread_frames(
        &self,
        thread_val: VALUE,
        ec_val: VALUE,
        trace_id: u64,
        timestamp: u64,
        iseq_logger: &mut crate::iseq_logger::IseqLogger,
        iseq_buffer: &mut std::collections::HashSet<u64>,
    ) {
        self.inner.record_thread_frames(
            thread_val,
            ec_val,
            trace_id,
            timestamp,
            iseq_logger,
            iseq_buffer,
        )
    }
}

// Get Ruby version string from the running interpreter using a Ruby call
unsafe fn get_ruby_version_string() -> String {
    // Use RUBY_VERSION constant which is available at runtime
    let version_sym = rb_sys::rb_intern("RUBY_VERSION\0".as_ptr() as *const c_char);
    let version_val = rb_sys::rb_const_get(rb_sys::rb_cObject, version_sym);

    // Convert VALUE to C string
    let version_ptr = rb_sys::rb_string_value_cstr(&version_val as *const _ as *mut _);
    let version_cstr = CStr::from_ptr(version_ptr);
    version_cstr.to_string_lossy().to_string()
}

// Parse version string to determine which structs to use
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

// Macro to dispatch function calls based on Ruby version
macro_rules! with_ruby_version {
    ($version:expr, $func:ident, $($args:expr),*) => {
        match $version {
            RubyVersion::Ruby315 => {
                use rbspy_ruby_structs::ruby_3_1_5 as ruby_structs;
                $func::<ruby_structs::rb_control_frame_struct,
                       ruby_structs::rb_execution_context_struct,
                       ruby_structs::rb_iseq_struct,
                       ruby_structs::rb_thread_t,
                       ruby_structs::RString>($($args),*)
            },
            RubyVersion::Ruby322 => {
                use rbspy_ruby_structs::ruby_3_2_5 as ruby_structs;
                $func::<ruby_structs::rb_control_frame_struct,
                       ruby_structs::rb_execution_context_struct,
                       ruby_structs::rb_iseq_struct,
                       ruby_structs::rb_thread_t,
                       ruby_structs::RString>($($args),*)
            },
            RubyVersion::Ruby330 => {
                use rbspy_ruby_structs::ruby_3_3_1 as ruby_structs;
                $func::<ruby_structs::rb_control_frame_struct,
                       ruby_structs::rb_execution_context_struct,
                       ruby_structs::rb_iseq_struct,
                       ruby_structs::rb_thread_t,
                       ruby_structs::RString>($($args),*)
            },
            RubyVersion::Unknown => {
                use rbspy_ruby_structs::ruby_3_1_5 as ruby_structs;
                $func::<ruby_structs::rb_control_frame_struct,
                       ruby_structs::rb_execution_context_struct,
                       ruby_structs::rb_iseq_struct,
                       ruby_structs::rb_thread_t,
                       ruby_structs::RString>($($args),*)
            }
        }
    };
}

// Helper macro for struct size calculations
macro_rules! get_struct_size {
    ($version:expr, $struct_type:ident) => {
        match $version {
            RubyVersion::Ruby315 => {
                std::mem::size_of::<rbspy_ruby_structs::ruby_3_1_5::$struct_type>()
            }
            RubyVersion::Ruby322 => {
                std::mem::size_of::<rbspy_ruby_structs::ruby_3_2_5::$struct_type>()
            }
            RubyVersion::Ruby330 => {
                std::mem::size_of::<rbspy_ruby_structs::ruby_3_3_1::$struct_type>()
            }
            RubyVersion::Unknown => {
                std::mem::size_of::<rbspy_ruby_structs::ruby_3_1_5::$struct_type>()
            }
        }
    };
}

pub(crate) use get_struct_size;
pub(crate) use with_ruby_version;
