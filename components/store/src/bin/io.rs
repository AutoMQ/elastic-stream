use std::{
    alloc::{self, Layout},
    error::Error,
    os::fd::AsRawFd,
};

use io_uring::{opcode, register, types, IoUring};

fn main() -> Result<(), Box<dyn Error>> {
    let mut uring = IoUring::builder()
        .dontfork()
        .setup_iopoll()
        .setup_sqpoll(2000)
        .setup_sqpoll_cpu(1)
        .setup_r_disabled()
        .build(1024)?;

    let mut probe = register::Probe::new();
    uring.submitter().register_probe(&mut probe)?;
    uring.submitter().register_iowq_max_workers(&mut [2, 2])?;
    uring.submitter().register_enable_rings()?;

    if !probe.is_supported(opcode::Fallocate64::CODE) {
        eprintln!("Fallocate is not supported");
        return Ok(());
    }

    let mut ring = IoUring::builder()
        .dontfork()
        .setup_attach_wq(uring.as_raw_fd())
        .build(1024)?;

    const LEN: i64 = 1024 * 4;

    let mut fd = 0;
    // Create file
    {
        let file_name = "/mnt/abc.txt".to_owned();
        let open_at_e = opcode::OpenAt::new(
            types::Fd(libc::AT_FDCWD),
            file_name.as_ptr() as *const libc::c_char,
        )
        .flags(libc::O_CREAT | libc::O_RDWR | libc::O_DIRECT)
        .mode(libc::S_IRWXU | libc::S_IRWXG)
        .build()
        .user_data(0);
        unsafe { ring.submission().push(&open_at_e)? };
        ring.submit_and_wait(1)?;
        let mut cq = ring.completion();
        if let Some(cqe) = cq.next() {
            println!("{cqe:?}");
            fd = cqe.result();
        }
    }

    // Fallocate to change file size
    {
        let fallocate_e = opcode::Fallocate64::new(types::Fd(fd), LEN)
            .mode(0)
            .offset64(0)
            .build()
            .user_data(1);
        unsafe { ring.submission().push(&fallocate_e)? };

        ring.submit_and_wait(1)?;

        let mut cq = ring.completion();
        for cqe in cq.by_ref() {
            println!("{cqe:?}");
        }
        cq.sync();
    }

    // Write 4KiB data
    {
        let alignment = 4096;
        let buf_size = 4096;

        let layout = Layout::from_size_align(buf_size, alignment)?;
        let ptr = unsafe { alloc::alloc(layout) };

        unsafe { libc::memset(ptr as *mut libc::c_void, 65, buf_size as libc::size_t) };

        let write_e = opcode::Write::new(types::Fd(fd), ptr, buf_size as u32)
            .offset(0)
            .build()
            .user_data(2);

        unsafe { uring.submission().push_multiple(&[write_e])? };
        uring.submit_and_wait(1)?;
        let mut cq = uring.completion();
        for cqe in cq.by_ref() {
            println!("{cqe:?}");
        }
        cq.sync();

        unsafe { alloc::dealloc(ptr, layout) };
    }

    // Read the written data
    {
        let alignment = 4096;
        let buf_size = 4096;

        let layout = Layout::from_size_align(buf_size, alignment)?;
        let ptr = unsafe { alloc::alloc_zeroed(layout) };

        let read_e = opcode::Read::new(types::Fd(fd), ptr, 2048)
            .offset(0)
            .build()
            .user_data(3);

        unsafe { uring.submission().push_multiple(&[read_e])? };
        uring.submit_and_wait(1)?;
        let mut cq = uring.completion();
        for cqe in cq.by_ref() {
            println!("{cqe:?}");
        }
        cq.sync();

        // Output the read data
        let data = unsafe { std::slice::from_raw_parts(ptr as *const u8, buf_size) };
        let actual = &data[0..2048];
        debug_assert!(
            actual.iter().all(|c| c == &65),
            "First 2048 bytes should be all 'A'"
        );

        unsafe { alloc::dealloc(ptr, layout) };
    }

    let result = unsafe { libc::close(fd) };
    debug_assert_eq!(0, result, "Failed to close FD");

    Ok(())
}
