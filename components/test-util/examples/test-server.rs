use std::{error::Error, time::Duration};
use tokio::time;

fn main() -> Result<(), Box<dyn Error>> {
    tokio_uring::start(async {
        let port = test_util::run_listener().await;
        println!("Listening {}", port);
        time::sleep(Duration::from_secs(300)).await;
        Ok(())
    })
}
