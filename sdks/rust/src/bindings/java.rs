use bytes::Bytes;
use jni::objects::{GlobalRef, JClass, JObject, JString, JValue, JValueGen};
use jni::sys::{jint, jlong, JNINativeInterface_, JNI_VERSION_1_8};
use jni::{JNIEnv, JavaVM};
use log::{error, info};
use std::alloc::Layout;
use std::cell::{OnceCell, RefCell};
use std::ffi::c_void;
use std::slice;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

use crate::{ClientError, Frontend, Stream, StreamOptions};

use super::cmd::Command;

static mut TX: OnceCell<mpsc::UnboundedSender<Command>> = OnceCell::new();

thread_local! {
    static JAVA_VM: RefCell<Option<Arc<JavaVM>>> = RefCell::new(None);
    static JENV: RefCell<Option<*mut jni::sys::JNIEnv>> = RefCell::new(None);
}

async fn process_command(cmd: Command<'_>) {
    match cmd {
        Command::CreateStream {
            front_end,
            replica,
            ack_count,
            retention,
            future,
        } => {
            process_create_stream_command(front_end, replica, ack_count, retention, future).await;
        }
        Command::GetFrontend { access_point, tx } => {
            process_get_frontend_command(access_point, tx);
        }
        Command::OpenStream {
            front_end,
            stream_id,
            epoch,
            future,
        } => {
            process_open_stream_command(front_end, stream_id, epoch, future).await;
        }
        Command::StartOffset { stream, future } => {
            process_start_offset_command(stream, future).await;
        }
        Command::NextOffset { stream, future } => {
            process_next_offset_command(stream, future).await;
        }
        Command::Append {
            stream,
            buf,
            future,
        } => {
            process_append_command(stream, buf, future).await;
        }
        Command::Read {
            stream,
            start_offset,
            end_offset,
            batch_max_bytes,
            future,
        } => {
            process_read_command(stream, start_offset, end_offset, batch_max_bytes, future).await;
        }
    }
}
async fn process_append_command(stream: &mut Stream, buf: Bytes, future: GlobalRef) {
    let result = stream.append(buf).await;
    match result {
        Ok(result) => {
            let base_offset = result.base_offset;
            JENV.with(|cell| {
                let mut env = unsafe { get_thread_local_jenv(cell) };
                let long_class = env.find_class("java/lang/Long").unwrap();
                let obj = env
                    .new_object(
                        long_class,
                        "(J)V",
                        &[jni::objects::JValueGen::Long(base_offset)],
                    )
                    .unwrap();
                unsafe { call_future_complete_method(env, future, obj) };
            });
        }
        Err(err) => {
            JENV.with(|cell| {
                let mut env = unsafe { get_thread_local_jenv(cell) };
                unsafe {
                    call_future_complete_exceptionally_method(&mut env, future, err.to_string())
                };
            });
        }
    };
}
async fn process_read_command(
    stream: &mut Stream,
    start_offset: i64,
    end_offset: i64,
    batch_max_bytes: i32,
    future: GlobalRef,
) {
    let result = stream.read(start_offset, end_offset, batch_max_bytes).await;
    match result {
        Ok(buffers) => {
            // Copy buffers to `DirectByteBuffer`
            let total = buffers.iter().map(|buf| buf.len()).sum();
            let layout = Layout::from_size_align(total, 1).expect("Bad alignment");
            let ptr = unsafe { std::alloc::alloc(layout) };
            let mut p = 0;
            buffers.iter().for_each(|buf| {
                unsafe { std::ptr::copy_nonoverlapping(buf.as_ptr(), ptr.add(p), buf.len()) };
                p += buf.len();
            });

            JENV.with(|cell| {
                let mut env = unsafe { get_thread_local_jenv(cell) };
                let obj = unsafe { env.new_direct_byte_buffer(ptr, total).unwrap() };
                unsafe { call_future_complete_method(env, future, JObject::from(obj)) };
            });
        }
        Err(err) => {
            JENV.with(|cell| {
                let mut env = unsafe { get_thread_local_jenv(cell) };
                unsafe {
                    call_future_complete_exceptionally_method(&mut env, future, err.to_string())
                };
            });
        }
    };
}

async fn process_start_offset_command(stream: &mut Stream, future: GlobalRef) {
    let result = stream.start_offset().await;
    match result {
        Ok(offset) => {
            JENV.with(|cell| {
                let mut env = unsafe { get_thread_local_jenv(cell) };

                let long_class = env.find_class("java/lang/Long").unwrap();
                let obj = env
                    .new_object(long_class, "(J)V", &[jni::objects::JValueGen::Long(offset)])
                    .unwrap();
                unsafe { call_future_complete_method(env, future, obj) };
            });
        }
        Err(err) => {
            JENV.with(|cell| {
                let mut env = unsafe { get_thread_local_jenv(cell) };
                unsafe {
                    call_future_complete_exceptionally_method(&mut env, future, err.to_string())
                };
            });
        }
    };
}

async fn process_next_offset_command(stream: &mut Stream, future: GlobalRef) {
    let result = stream.next_offset().await;
    match result {
        Ok(offset) => {
            JENV.with(|cell| {
                let mut env = unsafe { get_thread_local_jenv(cell) };
                let long_class = env.find_class("java/lang/Long").unwrap();
                let obj = env
                    .new_object(long_class, "(J)V", &[jni::objects::JValueGen::Long(offset)])
                    .unwrap();
                unsafe { call_future_complete_method(env, future, obj) };
            });
        }
        Err(err) => {
            JENV.with(|cell| {
                let mut env = unsafe { get_thread_local_jenv(cell) };
                unsafe {
                    call_future_complete_exceptionally_method(&mut env, future, err.to_string())
                };
            });
        }
    };
}

fn process_get_frontend_command(
    access_point: String,
    tx: oneshot::Sender<Result<Frontend, ClientError>>,
) {
    let result = Frontend::new(&access_point);
    if let Err(_e) = tx.send(result) {
        error!("Failed to dispatch JNI command to tokio-uring runtime");
    }
}

async fn process_open_stream_command(
    front_end: &mut Frontend,
    stream_id: u64,
    epoch: u64,
    future: GlobalRef,
) {
    let result = front_end.open(stream_id, epoch).await;
    match result {
        Ok(stream) => {
            let ptr = Box::into_raw(Box::new(stream)) as jlong;
            JENV.with(|cell| {
                let mut env = unsafe { get_thread_local_jenv(cell) };
                let stream_class = env
                    .find_class("com/automq/elasticstream/client/jni/Stream")
                    .unwrap();
                let obj = env
                    .new_object(stream_class, "(J)V", &[jni::objects::JValueGen::Long(ptr)])
                    .unwrap();
                unsafe { call_future_complete_method(env, future, obj) };
            });
        }
        Err(err) => {
            JENV.with(|cell| {
                let mut env = unsafe { get_thread_local_jenv(cell) };
                unsafe {
                    call_future_complete_exceptionally_method(&mut env, future, err.to_string())
                };
            });
        }
    };
}

async fn process_create_stream_command(
    front_end: &mut Frontend,
    replica: u8,
    ack_count: u8,
    retention: Duration,
    future: GlobalRef,
) {
    let options = StreamOptions {
        replica: replica,
        ack: ack_count,
        retention: retention,
    };
    let result = front_end.create(options).await;
    match result {
        Ok(stream_id) => {
            JENV.with(|cell| {
                let mut env = unsafe { get_thread_local_jenv(cell) };
                let long_class = env.find_class("java/lang/Long").unwrap();
                let obj = env
                    .new_object(
                        long_class,
                        "(J)V",
                        &[jni::objects::JValueGen::Long(stream_id as i64)],
                    )
                    .unwrap();
                unsafe { call_future_complete_method(env, future, obj) };
            });
        }
        Err(err) => {
            JENV.with(|cell| {
                let mut env = unsafe { get_thread_local_jenv(cell) };
                unsafe {
                    call_future_complete_exceptionally_method(&mut env, future, err.to_string())
                };
            });
        }
    };
}
/// # Safety
///
/// This function could be only called by java vm when onload this lib.
#[no_mangle]
pub unsafe extern "system" fn JNI_OnLoad(vm: JavaVM, _: *mut c_void) -> jint {
    let java_vm = Arc::new(vm);
    let (tx, mut rx) = mpsc::unbounded_channel();
    TX.set(tx).expect("Failed to set command channel sender");
    let _ = std::thread::Builder::new()
        .name("Runtime".to_string())
        .spawn(move || {
            JENV.with(|cell| {
                let env = java_vm.attach_current_thread_as_daemon().unwrap();
                *cell.borrow_mut() = Some(env.get_raw());
            });
            JAVA_VM.with(|cell| {
                *cell.borrow_mut() = Some(java_vm.clone());
            });
            tokio_uring::start(async move {
                loop {
                    match rx.recv().await {
                        Some(cmd) => {
                            tokio_uring::spawn(async move { process_command(cmd).await });
                        }
                        None => {
                            info!("JNI command channel is dropped");
                            break;
                        }
                    }
                }
            });
        });
    JNI_VERSION_1_8
}

/// # Safety
///
/// This function could be only called by java vm when unload this lib.
#[no_mangle]
pub unsafe extern "system" fn JNI_OnUnload(_: JavaVM, _: *mut c_void) {
    TX.take();
}

// Frontend

#[no_mangle]
pub unsafe extern "system" fn Java_com_automq_elasticstream_client_jni_Frontend_getFrontend(
    mut env: JNIEnv,
    _class: JClass,
    access_point: JString,
) -> jlong {
    let (tx_frontend, rx_frontend) = oneshot::channel();
    let command = env
        .get_string(&access_point)
        .map(|access_point| Command::GetFrontend {
            access_point: access_point.into(),
            tx: tx_frontend,
        });
    if let Ok(command) = command {
        match TX.get() {
            Some(tx) => match tx.send(command) {
                Ok(_) => match rx_frontend.blocking_recv() {
                    Ok(result) => match result {
                        Ok(frontend) => Box::into_raw(Box::new(frontend)) as jlong,
                        Err(err) => {
                            throw_exception(&mut env, &err.to_string());
                            0
                        }
                    },
                    Err(_) => {
                        error!("Failed to receive GetFrontend command response from tokio-uring runtime");
                        0
                    }
                },
                Err(_) => {
                    error!("Failed to dispatch GetFrontend command to tokio-uring runtime");
                    0
                }
            },
            None => {
                info!("JNI command channel was dropped. Ignore a GetFrontend request");
                0
            }
        }
    } else {
        info!("Failed to construct GetFrontend command. Ignore a GetFrontend request");
        0
    }
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_automq_elasticstream_client_jni_Frontend_freeFrontend(
    mut _env: JNIEnv,
    _class: JClass,
    ptr: *mut Frontend,
) {
    // Take ownership of the pointer by wrapping it with a Box
    let _ = Box::from_raw(ptr);
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_automq_elasticstream_client_jni_Frontend_create(
    mut env: JNIEnv,
    _class: JClass,
    ptr: *mut Frontend,
    replica: jint,
    ack: jint,
    retention_millis: jlong,
    future: JObject,
) {
    let command = env.new_global_ref(future).map(|future| {
        let front_end = &mut *ptr;
        Command::CreateStream {
            front_end: front_end,
            replica: replica as u8,
            ack_count: ack as u8,
            retention: Duration::from_millis(retention_millis as u64),
            future,
        }
    });
    if let Ok(command) = command {
        if let Some(tx) = TX.get() {
            if let Err(_e) = tx.send(command) {
                error!("Failed to dispatch CreateStream command to tokio-uring runtime");
            }
        } else {
            info!("JNI command channel was dropped. Ignore a CreateStream request");
        }
    } else {
        info!("Failed to construct CreateStream command");
    };
}
#[no_mangle]
pub unsafe extern "system" fn Java_com_automq_elasticstream_client_jni_Frontend_open(
    mut env: JNIEnv,
    _class: JClass,
    ptr: *mut Frontend,
    id: jlong,
    epoch: jlong,
    future: JObject,
) {
    debug_assert!(id >= 0, "Stream ID should be non-negative");
    let command = env.new_global_ref(future).map(|future| {
        let front_end = &mut *ptr;
        Command::OpenStream {
            front_end,
            stream_id: id as u64,
            epoch: epoch as u64,
            future: future,
        }
    });
    if let Ok(command) = command {
        if let Some(tx) = TX.get() {
            if let Err(_e) = tx.send(command) {
                error!("Failed to dispatch OpenStream command to tokio-uring runtime");
            }
        } else {
            info!("JNI command channel was dropped. Ignore an OpenStream request");
        }
    } else {
        info!("Failed to construct OpenStream command");
    }
}

// Stream

#[no_mangle]
pub unsafe extern "system" fn Java_com_automq_elasticstream_client_jni_Stream_freeStream(
    env: JNIEnv,
    _class: JClass,
    ptr: *mut Stream,
) {
    // Take ownership of the pointer by wrapping it with a Box
    let _ = Box::from_raw(ptr);
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_automq_elasticstream_client_jni_Stream_minOffset(
    mut env: JNIEnv,
    _class: JClass,
    ptr: *mut Stream,
    future: JObject,
) {
    let command = env.new_global_ref(future).map(|future| {
        let stream = &mut *ptr;
        Command::StartOffset {
            stream: stream,
            future: future,
        }
    });
    if let Ok(command) = command {
        if let Some(tx) = TX.get() {
            if let Err(_e) = tx.send(command) {
                error!("Failed to dispatch StartOffset command to tokio-uring runtime");
            }
        } else {
            info!("JNI command channel was dropped. Ignore a StartOffset request");
        }
    } else {
        info!("Failed to construct StartOffset command.");
    }
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_automq_elasticstream_client_jni_Stream_maxOffset(
    mut env: JNIEnv,
    _class: JClass,
    ptr: *mut Stream,
    future: JObject,
) {
    let command = env.new_global_ref(future).map(|future| {
        let stream = &mut *ptr;
        Command::NextOffset {
            stream: stream,
            future: future,
        }
    });
    if let Ok(command) = command {
        if let Some(tx) = TX.get() {
            if let Err(_e) = tx.send(command) {
                error!("Failed to dispatch NextOffset command to tokio-uring runtime");
            }
        } else {
            info!("JNI command channel was dropped. Ignore a NextOffset request");
        }
    } else {
        info!("Failed to construct NextOffset command");
    }
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_automq_elasticstream_client_jni_Stream_append(
    mut env: JNIEnv,
    _class: JClass,
    ptr: *mut Stream,
    data: JObject,
    future: JObject,
) {
    let buf = env.get_direct_buffer_address((&data).into());
    let len = env.get_direct_buffer_capacity((&data).into());
    let future = env.new_global_ref(future);
    let command: Result<Command, ()> = match (buf, len, future) {
        (Ok(buf), Ok(len), Ok(future)) => {
            let stream = &mut *ptr;
            let buf = Bytes::copy_from_slice(slice::from_raw_parts(buf, len));
            Ok(Command::Append {
                stream,
                buf,
                future,
            })
        }
        _ => Err(()),
    };
    if let Ok(command) = command {
        if let Some(tx) = TX.get() {
            if let Err(_e) = tx.send(command) {
                error!("Failed to dispatch Append command to tokio-uring runtime");
            }
        } else {
            info!("JNI command channel was dropped. Ignore an Append request");
        }
    } else {
        info!("Failed to construct Append command");
    }
}

#[no_mangle]
pub unsafe extern "system" fn Java_com_automq_elasticstream_client_jni_Stream_read(
    env: JNIEnv,
    _class: JClass,
    ptr: *mut Stream,
    start_offset: jlong,
    end_offset: jlong,
    batch_max_bytes: jint,
    future: JObject,
) {
    let command = env.new_global_ref(future).map(|future| {
        let stream = &mut *ptr;
        Command::Read {
            stream: stream,
            start_offset: start_offset,
            end_offset: end_offset,
            batch_max_bytes: batch_max_bytes,
            future: future,
        }
    });
    if let Ok(command) = command {
        if let Some(tx) = TX.get() {
            if let Err(_e) = tx.send(command) {
                error!("Failed to dispatch Read command to tokio-uring runtime");
            }
        } else {
            info!("JNI command channel was dropped. Ignore a Read request");
        }
    } else {
        info!("Failed to construct Read command");
    }
}

unsafe fn call_future_complete_method(mut env: JNIEnv, future: GlobalRef, obj: JObject) {
    let s = JValueGen::from(obj);
    let _ = env
        .call_method(future, "complete", "(Ljava/lang/Object;)Z", &[s.borrow()])
        .unwrap();
}

unsafe fn call_future_complete_exceptionally_method(
    env: &mut JNIEnv,
    future: GlobalRef,
    err_msg: String,
) {
    let exception_class = env.find_class("java/lang/Exception").unwrap();
    let message = env.new_string(err_msg).unwrap();
    let obj = env
        .new_object(
            exception_class,
            "(Ljava/lang/String;)V",
            &[JValue::Object(message.as_ref())],
        )
        .unwrap();
    let s = JValueGen::from(obj);
    let _ = env
        .call_method(
            future,
            "completeExceptionally",
            "(Ljava/lang/Throwable;)Z",
            &[s.borrow()],
        )
        .unwrap();
}

unsafe fn get_thread_local_jenv(cell: &RefCell<Option<*mut *const JNINativeInterface_>>) -> JNIEnv {
    let env_ptr = cell.borrow().unwrap();
    JNIEnv::from_raw(env_ptr).unwrap()
}

fn throw_exception(env: &mut JNIEnv, msg: &str) {
    let _ = env.exception_clear();
    env.throw_new("java/lang/Exception", msg)
        .expect("Couldn't throw exception");
}
