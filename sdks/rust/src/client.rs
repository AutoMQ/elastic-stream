use model::stream::Stream;

use crate::{client_error::ClientError, session_manager::SessionManager};

pub struct Client {
    session_manager: SessionManager,
}

impl Client {
    pub async fn create_stream(&self) -> Result<Stream, ClientError> {
        self.session_manager.create_stream().await
    }
}

#[cfg(test)]
mod tests {

    use std::time::Duration;

    use minitrace::prelude::*;

    #[trace("do something")]
    fn do_something(d: Duration) {
        std::thread::sleep(d);
    }

    #[trace("do something async")]
    async fn do_something_async(d: Duration) {
        tokio::time::sleep(d).await;
    }

    #[tokio::test]
    async fn test_mini_trace() {
        let (mut root, collector) = Span::root("root");
        root.add_properties(|| vec![("k1", "v1".to_string()), ("k2", "v2".to_string())]);
        {
            let mut _child_span = Span::enter_with_parent("child span", &root);
            _child_span.add_property(|| ("key", "value".to_owned()));
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;

            {
                let _g = _child_span.set_local_parent();
                do_something(Duration::from_millis(100));
                do_something_async(Duration::from_millis(300)).await;
            }
        }
        drop(root);

        let records = collector.collect().await;
        dbg!(records);
    }
}
