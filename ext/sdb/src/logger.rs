use fast_log::config::Config;

const FAST_LOG_CHAN_LEN: usize = 100_000;
const ISEQS_BUFFER_SIZE: usize = 1_000_000;

pub(crate) fn init_logger() -> &'static fast_log::Logger {
    // TODO: check why unwrap may panic in rspec
    // reproduce: RUST_BACKTRACE=1 bundle exec rspec spec/sdb_spec.rb
    fast_log::init(
        Config::new()
            .file("sdb.log")
            .chan_len(Some(FAST_LOG_CHAN_LEN)),
    )
    .unwrap()
}

pub struct Logger {
    buffer: [u64; ISEQS_BUFFER_SIZE],
    buffer_size: usize,
    buffer_index: usize,
}

impl Logger {
    pub fn new() -> Self {
        Logger {
            buffer: [0; ISEQS_BUFFER_SIZE],
            buffer_size: ISEQS_BUFFER_SIZE,
            buffer_index: 0,
        }
    }

    #[inline]
    pub fn push(&mut self, item: u64) {
        if self.buffer_index < self.buffer_size {
            self.buffer[self.buffer_index] = item;
            self.buffer_index += 1;
        } else {
            log::info!("[{}][stack_frames]{:?}", std::process::id(), self.buffer);
            self.buffer_index = 0;
        }
    }

    #[inline]
    pub fn push_seperator(&mut self) {
        self.push(u64::MAX);
        self.push(u64::MAX);
    }

    #[inline]
    pub fn flush(&mut self) {
        log::info!(
            "[{}][stack_frames]{:?}",
            std::process::id(),
            &self.buffer[..self.buffer_index]
        );
        self.buffer_index = 0;
        log::logger().flush();
    }

    #[inline]
    pub fn log(&mut self, str: &str) {
        log::info!("[{}]{}", std::process::id(), str);
    }
}
