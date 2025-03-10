use flexi_logger::{FileSpec, Logger, LoggerHandle, WriteMode};
use log::info;

pub(crate) fn init_logger() -> LoggerHandle {
    let logger = Logger::try_with_str("info")
        .unwrap()
        .log_to_file(
            FileSpec::default()
                .directory("logs")
                .basename("sdb")
                .suffix("log"),
        )
        .write_mode(WriteMode::Direct)
        .format(flexi_logger::default_format)
        .start()
        .unwrap();

    info!("Logger initialized");
    logger
}
