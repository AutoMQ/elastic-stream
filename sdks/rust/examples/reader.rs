use std::error::Error;

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn Error>> {
    let access_point = "localhost:80";
    let stream_id = 1;

    Ok(())
}
