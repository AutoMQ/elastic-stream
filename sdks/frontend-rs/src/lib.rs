pub mod append_result;
pub mod bindings;
pub mod error;
pub mod frontend;
pub mod log;
pub mod stream;
pub mod stream_options;
mod time_format;

pub use crate::append_result::AppendResult;
pub(crate) use crate::bindings::stopwatch::Stopwatch;
pub use crate::error::ClientError;
pub use crate::frontend::Frontend;
pub use crate::log::init_log;
pub use crate::stream::Stream;
pub use crate::stream_options::StreamOptions;

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::error::Error;
    use std::rc::Rc;
    use std::time::Duration;

    #[test]
    fn test_semaphore() -> Result<(), Box<dyn Error>> {
        tokio_uring::start(async {
            let start = minstant::Instant::now();
            let semaphore = Rc::new(local_sync::semaphore::Semaphore::new(3));
            let cnt = Rc::new(RefCell::new(0));

            let cnt_cloned = Rc::clone(&cnt);
            let handle = tokio_uring::spawn(async move {
                loop {
                    if start.elapsed().as_secs() > 2 {
                        println!("2 seconds has elapsed, stop.");
                        break;
                    }

                    if *cnt_cloned.borrow() > 10 {
                        break;
                    }

                    match Rc::clone(&semaphore).acquire_owned().await {
                        Ok(permit) => {
                            let cnt_ = Rc::clone(&cnt_cloned);
                            tokio_uring::spawn(async move {
                                *cnt_.borrow_mut() += 1;
                                tokio::time::sleep(Duration::from_millis(100)).await;
                                drop(permit);
                            });
                        }
                        Err(_e) => {
                            break;
                        }
                    }
                }
            });
            let _ = handle.await;
            assert!(*cnt.borrow() > 10);
            Ok(())
        })
    }
}
