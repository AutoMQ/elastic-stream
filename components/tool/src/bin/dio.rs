use io_uring::{self, opcode, types, IoUring};
use std::{
    alloc::{self, Layout},
    error::Error,
};

const IO_DEPTH: u32 = 4096;

const FILE_SIZE: i64 = 1i64 * 1024 * 1024 * 1024;

fn main() -> Result<(), Box<dyn Error>> {
    println!("PID: {}", std::process::id());

    let mut control_ring = io_uring::IoUring::builder().dontfork().build(32)?;
    let file_path = "/data/data0";
    let sqe = opcode::OpenAt::new(types::Fd(libc::AT_FDCWD), file_path.as_ptr())
        .flags(libc::O_CREAT | libc::O_RDWR | libc::O_DIRECT | libc::O_DSYNC)
        .mode(libc::S_IRWXU | libc::S_IRWXG)
        .build()
        .user_data(0);
    unsafe { control_ring.submission().push(&sqe) }?;
    control_ring.submit_and_wait(1)?;
    let fd = {
        let mut cq = control_ring.completion();
        let cqe = cq.next().unwrap();
        debug_assert_eq!(0, cqe.user_data(), "user-data is inconsistent");
        cq.sync();
        cqe.result()
    };
    if fd < 0 {
        panic!("Failed to open {file_path}, errno: {}", -fd);
    }
    println!("Opened {file_path} with fd={fd}");

    let sqe = opcode::Fallocate64::new(types::Fd(fd), FILE_SIZE)
        .offset(0)
        .mode(libc::FALLOC_FL_ZERO_RANGE)
        .build()
        .user_data(1);
    unsafe { control_ring.submission().push(&sqe) }?;
    control_ring.submit_and_wait(1)?;
    {
        let mut cq = control_ring.completion();
        let cqe = cq.next().unwrap();
        debug_assert_eq!(1, cqe.user_data(), "user-data is inconsistent");
        cq.sync();
        if cqe.result() >= 0 {
            println!("Fallocate File[{file_path}, FD={fd}] to {} OK", FILE_SIZE);
        } else {
            panic!("Failed to fallocate, errno: {}", -cqe.result());
        }
    }

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
    submitter.register_files(&[fd])?;
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
