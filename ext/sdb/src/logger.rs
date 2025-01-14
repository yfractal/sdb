use fast_log::config::Config;
use fast_log::Logger;

const FAST_LOG_CHAN_LEN: usize = 100_000;

pub(crate) fn init_logger() -> &'static Logger {
    // TODO: check why unwrap may panic in rspec
    // reproduce: RUST_BACKTRACE=1 bundle exec rspec spec/sdb_spec.rb
    fast_log::init(
        Config::new()
            .file("sdb.log")
            .chan_len(Some(FAST_LOG_CHAN_LEN)),
    )
    .unwrap()
}
