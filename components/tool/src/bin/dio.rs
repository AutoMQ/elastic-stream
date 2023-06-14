use clap::Parser;
use io_uring::{self, opcode, register, types, IoUring, Parameters};
use std::{
    alloc::{self, Layout},
    error::Error,
    ffi::CString,
    ops::IndexMut,
};

fn check_io_uring(probe: &register::Probe, params: &Parameters) {
    if !params.is_feature_sqpoll_nonfixed() {
        panic!("io_uring feature: IORING_FEAT_SQPOLL_NONFIXED is required. Current kernel version is too old");
    }
    println!("io_uring has feature IORING_FEAT_SQPOLL_NONFIXED");

    // io_uring should support never dropping completion events.
    if !params.is_feature_nodrop() {
        panic!("io_uring setup: IORING_SETUP_CQ_NODROP is required.");
    }
    println!("io_uring has feature IORING_SETUP_CQ_NODROP");

    let codes = [
        opcode::OpenAt::CODE,
        opcode::Fallocate64::CODE,
        opcode::Write::CODE,
        opcode::Read::CODE,
        opcode::Close::CODE,
        opcode::UnlinkAt::CODE,
    ];
    for code in &codes {
        if !probe.is_supported(*code) {
            eprintln!("opcode {} is not supported", *code);
        }
    }
}

/// Frequency of each latency slot
/// # Arguments
/// `histogram` - Histogram of table
/// `value` - An observed record in nano seconds
fn observe(histogram: &mut [usize], value: u64) {
    // target slot index
    let mut target = (value / 1000 / 1000) as usize;
    if target >= histogram.len() {
        target = histogram.len() - 1;
    }

    *histogram.index_mut(target) += 1;
}

fn print_histogram(stats: &[usize]) {
    stats.iter().enumerate().for_each(|(idx, n)| {
        if *n != 0 {
            println!("[{}ms, {}ms): {n}", idx, idx + 1);
        }
    });
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = tool::Args::parse();
    println!("PID: {}", std::process::id());
    if minstant::is_tsc_available() {
        println!("TSC is available");
    } else {
        eprintln!("TSC is NOT available");
    }

    let file_size = args.size * 1024 * 1024 * 1024;

    let mut control_ring = io_uring::IoUring::builder()
        .dontfork()
        .setup_r_disabled()
        .build(32)?;
    let mut probe = register::Probe::new();
    let submitter = control_ring.submitter();
    submitter.register_probe(&mut probe)?;
    submitter.register_enable_rings()?;
    check_io_uring(&probe, control_ring.params());

    let file_path = args.path;
    let c_file_path = CString::new(file_path.clone()).unwrap();
    let sqe = opcode::OpenAt::new(types::Fd(libc::AT_FDCWD), c_file_path.as_ptr())
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

    let sqe = opcode::Fallocate64::new(types::Fd(fd), file_size as libc::off64_t)
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
            println!("Fallocate File[{file_path}, FD={fd}] to {} OK", file_size);
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
        .build(args.qd)?;

    let alignment = args.bs as usize;
    let buf_size = alignment * 4;

    let layout = Layout::from_size_align(buf_size, alignment)?;
    let ptr = unsafe { alloc::alloc(layout) };

    unsafe { libc::memset(ptr as *mut libc::c_void, 65, buf_size as libc::size_t) };

    let bufs = [libc::iovec {
        iov_base: ptr as *mut libc::c_void,
        iov_len: buf_size as libc::size_t,
    }];

    let mut probe = register::Probe::new();
    let submitter = uring.submitter();
    submitter.register_buffers(&bufs)?;
    submitter.register_probe(&mut probe)?;
    submitter.register_files(&[fd])?;
    submitter.register_iowq_max_workers(&mut [2, 2])?;

    submitter.register_enable_rings()?;
    check_io_uring(&probe, uring.params());

    const HISTOGRAM_N: usize = 101;

    let mut histogram = [0usize; HISTOGRAM_N];

    let mut writes = 0;
    let mut offset = 0;
    let anchor = minstant::Anchor::new();
    loop {
        loop {
            if writes >= args.qd {
                break;
            }

            if offset >= file_size {
                break;
            }

            let write_sqe =
                opcode::WriteFixed::new(types::Fixed(0), ptr as *const u8, buf_size as u32, 0)
                    .offset(offset as libc::off64_t)
                    .build()
                    .user_data(minstant::Instant::now().as_unix_nanos(&anchor));
            offset += buf_size;
            unsafe { uring.submission().push(&write_sqe) }?;
            writes += 1;
        }

        let _ = uring.submit_and_wait(1)?;

        let mut cq = uring.completion();
        loop {
            if cq.is_empty() {
                break;
            }

            #[allow(clippy::while_let_on_iterator)]
            while let Some(entry) = cq.next() {
                writes -= 1;
                let start = entry.user_data();
                let elapsed = minstant::Instant::now().as_unix_nanos(&anchor) - start;
                observe(&mut histogram, elapsed);
            }
            cq.sync();
        }

        if offset >= file_size && writes == 0 {
            println!("All writes are completed");
            break;
        }
    }

    print_histogram(&histogram);

    for buf in &bufs {
        unsafe { alloc::dealloc(buf.iov_base as *mut u8, layout) };
    }

    Ok(())
}
