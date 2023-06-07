use std::str::FromStr;

use log4rs::{
    append::{
        console::ConsoleAppender,
        rolling_file::{
            policy::compound::{
                roll::fixed_window::FixedWindowRoller, trigger::size::SizeTrigger, CompoundPolicy,
            },
            RollingFileAppender,
        },
    },
    config::{Appender, Logger, Root},
    encode::pattern::PatternEncoder,
    Config,
};

pub fn init_log() {
    let console_appender = ConsoleAppender::builder()
        .encoder(Box::new(PatternEncoder::new(
            "{d(%Y-%m-%d %H:%M:%S%.3f)} | {({l}):5.5} | {f}:{L} — {m}{n}",
        )))
        .build();

    // 16 MiB
    let file_size = 1024 * 1024 * 16;

    let client_roller = FixedWindowRoller::builder()
        .build("logs/archive/client.log.{}", 10)
        .expect("Failed to build fixed window roller for client");

    let client_appender = RollingFileAppender::builder()
        .encoder(Box::new(PatternEncoder::new(
            "{d(%Y-%m-%d %H:%M:%S%.3f)} | {({l}):5.5} | {f}:{L} — {m}{n}",
        )))
        .build(
            "logs/client.log",
            Box::new(CompoundPolicy::new(
                Box::new(SizeTrigger::new(file_size)),
                Box::new(client_roller),
            )),
        )
        .expect("Failed to build rolling file appender for client");

    let replication_roller = FixedWindowRoller::builder()
        .build("logs/archive/replication.log.{}", 10)
        .expect("Failed to build fixed window roller for replication");

    let replication_appender = RollingFileAppender::builder()
        .encoder(Box::new(PatternEncoder::new(
            "{d(%Y-%m-%d %H:%M:%S%.3f)} | {({l}):5.5} | {f}:{L} — {m}{n}",
        )))
        .build(
            "logs/replication.log",
            Box::new(CompoundPolicy::new(
                Box::new(SizeTrigger::new(file_size)),
                Box::new(replication_roller),
            )),
        )
        .expect("Failed to build rolling file appender for replication");

    let level = log::LevelFilter::from_str(&std::env::var("ES_SDK_LOG").unwrap_or_default())
        .unwrap_or(log::LevelFilter::Warn);

    let config = Config::builder()
        .appender(Appender::builder().build("stdout", Box::new(console_appender)))
        .appender(Appender::builder().build("client", Box::new(client_appender)))
        .appender(Appender::builder().build("replication", Box::new(replication_appender)))
        .logger(
            Logger::builder()
                .appender("client")
                .additive(false)
                .build("client", level),
        )
        .logger(
            Logger::builder()
                .appender("client")
                .additive(false)
                .build("codec", level),
        )
        .logger(
            Logger::builder()
                .appender("client")
                .additive(false)
                .build("transport", level),
        )
        .logger(
            Logger::builder()
                .appender("replication")
                .additive(false)
                .build("replication", level),
        )
        .logger(
            Logger::builder()
                .appender("replication")
                .additive(false)
                .build("frontend", level),
        )
        .build(Root::builder().appender("stdout").build(level))
        .expect("Failed to build log4rs config");

    log4rs::init_config(config).expect("Failed to init log4rs config");
}
