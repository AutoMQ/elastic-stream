
use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[arg(short, long, default_value_t = String::from("/data/data0"))]
    pub path: String,

    /// Queue depth
    #[arg(short, long, default_value_t = 32)]
    pub qd: u32,

    /// Block size
    #[arg(short, long, default_value_t = 4096)]
    pub bs: u16,

    /// File size in Gigabytes
    #[arg(short, long, default_value_t = 1)]
    pub size: usize,
}