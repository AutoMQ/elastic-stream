use io_uring::{self, opcode, types, IoUring};
use nix::fcntl;
use std::{
    alloc::{self, Layout},
    error::Error,
    fs::OpenOptions,
    os::{fd::AsRawFd, unix::prelude::OpenOptionsExt},
};

const IO_DEPTH: u32 = 4096;

const FILE_SIZE: i64 = 10i64 * 1024 * 1024 * 1024;

fn main() -> Result<(), Box<dyn Error>> {
    println!("PID: {}", std::process::id());

    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .read(true)
        .custom_flags(libc::O_DIRECT | libc::O_NONBLOCK)
        .open("/data/data0")?;

    fcntl::fallocate(
        file.as_raw_fd(),
        fcntl::FallocateFlags::empty(),
        0,
        FILE_SIZE,
    )?;

    let mut uring = IoUring::builder()
        .setup_iopoll()
        .setup_sqpoll(2000)
        .setup_sqpoll_cpu(1)
        .dontfork()
        .setup_r_disabled()
        .build(IO_DEPTH)?;

    let alignment = 4096;
    let buf_size = 4096 * 4;

    let layout = Layout::from_size_align(buf_size, alignment)?;
    let ptr = unsafe { alloc::alloc(layout) };

    unsafe { libc::memset(ptr as *mut libc::c_void, 65, buf_size as libc::size_t) };

    let bufs = [libc::iovec {
        iov_base: ptr as *mut libc::c_void,
        iov_len: buf_size as libc::size_t,
    }];

    let submitter = uring.submitter();
    submitter.register_buffers(&bufs)?;
    submitter.register_files(&[file.as_raw_fd()])?;
    submitter.register_iowq_max_workers(&mut [2, 2])?;

    submitter.register_enable_rings()?;

    const LATENCY_N: usize = 128;
    let mut latency = [0u16; LATENCY_N];
    let mut latency_index = 0_usize;

    let mut writes = 0;
    let mut offset = 0;
    let mut seq = 0;
    loop {
        loop {
            if writes >= IO_DEPTH {
                break;
            }

            if offset >= FILE_SIZE {
                break;
            }

            let write_sqe =
                opcode::WriteFixed::new(types::Fixed(0), ptr as *const u8, buf_size as u32, 0)
                    .offset(offset)
                    .build()
                    .user_data(seq);
            seq += 1;
            offset += buf_size as i64;
            unsafe { uring.submission().push(&write_sqe) }?;
            writes += 1;
        }

        let start = std::time::Instant::now();
        let _ = uring.submit_and_wait(1)?;
        // calculate latency
        let elapsed = start.elapsed().as_micros();
        latency[latency_index] = elapsed as u16;
        latency_index += 1;
        if latency_index + 1 >= LATENCY_N {
            latency_index = 0;
            let sum: u64 = latency.iter().map(|v| *v as u64).sum();
            println!("AVG submit_and_wait latency: {}us", sum / LATENCY_N as u64);
        }

        let mut cq = uring.completion();
        loop {
            if cq.is_empty() {
                break;
            }

            while let Some(_entry) = cq.next() {
                writes -= 1;
            }
            cq.sync();
        }

        if offset >= FILE_SIZE && writes == 0 {
            println!("All writes are completed");
            break;
        }
    }

    for buf in &bufs {
        unsafe { alloc::dealloc(buf.iov_base as *mut u8, layout) };
    }

    Ok(())
}
