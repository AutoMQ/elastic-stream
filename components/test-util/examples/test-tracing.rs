use std::error::Error;

use tracing::{event, instrument, Level};

fn main() -> Result<(), Box<dyn Error>> {
    let file_appender = tracing_appender::rolling::hourly("logs", "tracing.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_file(true)
        .with_line_number(true)
        .with_target(false)
        .with_writer(non_blocking)
        .init();

    tokio_uring::start(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            foo(42).await;
        }
    })
}

#[instrument]
async fn foo(n: usize) -> usize {
    event!(Level::INFO, "Got {}", n);
    n
}
