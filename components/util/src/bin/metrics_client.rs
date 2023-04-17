use hyper::{body::HttpBody, Client};
use tokio::{
    io::{stdout, AsyncWriteExt},
    time,
};

// A simple type alias so as to DRY.
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
#[tokio::main]
async fn main() -> Result<()> {
    loop {
        time::sleep(time::Duration::from_millis(1000)).await;
        let client = Client::new();
        let uri = "http://127.0.0.1:9898/metrics".parse()?;
        let mut resp = client.get(uri).await?;
        while let Some(chunk) = resp.body_mut().data().await {
            stdout().write_all(&chunk?).await?;
        }
    }
}
