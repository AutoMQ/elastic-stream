#[cfg(test)]
mod tests {
    use std::error::Error;

    use log::trace;

    #[test]
    fn test_oneshot() -> Result<(), Box<dyn Error>> {
        test_util::try_init_log();
        tokio_uring::start(async move {
            let (tx, rx) = local_sync::oneshot::channel();
            tokio_uring::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                tx.send(1).unwrap();
            });
            trace!("Received {}", rx.await?);
            Ok(())
        })
    }

    #[test]
    fn test_mpsc() -> Result<(), Box<dyn Error>> {
        tokio_uring::start(async move {

            let (tx, mut rx) = local_sync::mpsc::unbounded::channel();
            for i in 0..10 {
                let tx = tx.clone();
                tokio_uring::spawn(async move {
                    tx.send(i).expect("Channel should not be closed");
                });
            }

            for _ in 0..10 {
                trace!("Received {}", rx.recv().await.expect("Should receive a value"));
            }

            Ok(())
        })
    }
}
