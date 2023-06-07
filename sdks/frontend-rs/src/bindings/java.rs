use bytes::Bytes;
use jni::objects::{GlobalRef, JClass, JMethodID, JObject, JString, JValue, JValueGen};
use jni::sys::{jint, jlong, JNINativeInterface_, JNI_VERSION_1_8};
use jni::{JNIEnv, JavaVM};
use log::{error, info, trace};
use std::cell::{OnceCell, RefCell};
use std::ffi::c_void;
use std::slice;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

use crate::{ClientError, Frontend, Stopwatch, Stream, StreamOptions};

use super::cmd::Command;

static mut TX: OnceCell<mpsc::UnboundedSender<Command>> = OnceCell::new();
// TODO: Add exception class cache
static mut STREAM_CLASS_CACHE: OnceCell<GlobalRef> = OnceCell::new();
static mut STREAM_CTOR_CACHE: OnceCell<JMethodID> = OnceCell::new();
static mut JLONG_CLASS_CACHE: OnceCell<GlobalRef> = OnceCell::new();
static mut JLONG_CTOR_CACHE: OnceCell<JMethodID> = OnceCell::new();
static mut VOID_CLASS_CACHE: OnceCell<GlobalRef> = OnceCell::new();
static mut VOID_CTOR_CACHE: OnceCell<JMethodID> = OnceCell::new();
static mut FUTURE_COMPLETE_CACHE: OnceCell<JMethodID> = OnceCell::new();
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
        Command::CloseStream { stream, future } => {
            process_close_stream_command(stream, future).await;
        }
    }
}
async fn process_close_stream_command(stream: &mut Stream, future: GlobalRef) {
    trace!("Start processing close command");
    let result = stream.close().await;
    match result {
        Ok(_) => {
            complete_future_with_void(future);
        }
        Err(err) => {
            complete_future_with_error(future, err);
        }
    };
    trace!("Close command finished");
}

async fn process_append_command(stream: &mut Stream, buf: Bytes, future: GlobalRef) {
    trace!("Start processing append command");
    let result = stream.append(buf).await;
    match result {
        Ok(result) => {
            let base_offset = result.base_offset;
            complete_future_with_jlong(future, base_offset);
        }
        Err(err) => {
            complete_future_with_error(future, err);
        }
    };
    trace!("Append command finished");
}
async fn process_read_command(
    stream: &mut Stream,
    start_offset: i64,
    end_offset: i64,
    batch_max_bytes: i32,
    future: GlobalRef,
) {
    trace!("Start processing read command");
    let result = stream.read(start_offset, end_offset, batch_max_bytes).await;
    match result {
        Ok(buffers) => {
            let total: usize = buffers.iter().map(|buf| buf.len()).sum();
            JENV.with(|cell| {
                let env = get_thread_local_jenv(cell);
                let byte_array = env.new_byte_array(total as i32);
                let mut p: usize = 0;
                if let Ok(byte_array) = byte_array {
                    {
                        buffers.iter().for_each(|buf| {
                            let slice = buf.as_ref();
                            let slice: &[i8] = unsafe {
                                std::slice::from_raw_parts(slice.as_ptr() as *const i8, slice.len())
                            };
                            let _ = env.set_byte_array_region(&byte_array, p as i32, slice);
                            p += buf.len();
                        });
                    }
                    call_future_complete_method(env, future, JObject::from(byte_array));
                } else {
                    complete_future_with_error(
                        future,
                        ClientError::Internal("Failed to create byte array".to_string()),
                    );
                    error!("Failed to create byte array");
                }
            });
        }
        Err(err) => {
            complete_future_with_error(future, err);
        }
    };
    trace!("Read command finished");
}

async fn process_start_offset_command(stream: &mut Stream, future: GlobalRef) {
    trace!("Start processing start_offset command");
    let result = stream.start_offset().await;
    match result {
        Ok(offset) => {
            complete_future_with_jlong(future, offset);
        }
        Err(err) => {
            complete_future_with_error(future, err);
        }
    };
    trace!("Start_offset command finished");
}

async fn process_next_offset_command(stream: &mut Stream, future: GlobalRef) {
    trace!("Start processing next_offset command");
    let result = stream.next_offset().await;
    match result {
        Ok(offset) => {
            complete_future_with_jlong(future, offset);
        }
        Err(err) => {
            complete_future_with_error(future, err);
        }
    };
    trace!("Next_offset command finished");
}

fn process_get_frontend_command(
    access_point: String,
    tx: oneshot::Sender<Result<Frontend, ClientError>>,
) {
    trace!("Start processing get_frontend command");
    let result = Frontend::new(&access_point);
    if let Err(_e) = tx.send(result) {
        error!("Failed to dispatch JNI command to tokio-uring runtime");
    }
    trace!("Get_frontend command finished");
}

async fn process_open_stream_command(
    front_end: &mut Frontend,
    stream_id: u64,
    epoch: u64,
    future: GlobalRef,
) {
    trace!("Start processing open_stream command");
    let result = front_end.open(stream_id, epoch).await;
    match result {
        Ok(stream) => {
            let ptr = Box::into_raw(Box::new(stream)) as jlong;
            complete_future_with_stream(future, ptr);
        }
        Err(err) => {
            complete_future_with_error(future, err);
        }
    };
    trace!("Open_stream command finished");
}

async fn process_create_stream_command(
    front_end: &mut Frontend,
    replica: u8,
    ack_count: u8,
    retention: Duration,
    future: GlobalRef,
) {
    trace!("Start processing create_stream command");
    let options = StreamOptions {
        replica,
        ack: ack_count,
        retention,
    };
    let result = front_end.create(options).await;
    match result {
        Ok(stream_id) => {
            complete_future_with_jlong(future, stream_id as i64);
        }
        Err(err) => {
            complete_future_with_error(future, err);
        }
    };
    trace!("Create_stream command finished");
}
/// # Safety
///
/// This function could be only called by java vm when onload this lib.
#[no_mangle]
pub extern "system" fn JNI_OnLoad(vm: JavaVM, _: *mut c_void) -> jint {
    crate::init_log();
    let java_vm = Arc::new(vm);
    let (tx, mut rx) = mpsc::unbounded_channel();
    // # Safety
    // Set OnceCell, expected to be safe.
    if unsafe { TX.set(tx) }.is_err() {
        error!("Failed to set command channel sender");
    }
    let _ = std::thread::Builder::new()
        .name("Runtime".to_string())
        .spawn(move || {
            trace!("JNI Runtime thread started");
            JENV.with(|cell| {
                if let Ok(mut env) = java_vm.attach_current_thread_as_daemon() {
                    *cell.borrow_mut() = Some(env.get_raw());
                    let stream_path = "com/automq/elasticstream/client/jni/Stream";
                    let void_path = "java/lang/Void";
                    let jlong_path = "java/lang/Long";
                    let completable_future_path = "java/util/concurrent/CompletableFuture";
                    let stream_class = env.find_class(stream_path).unwrap();
                    let stream_class: GlobalRef = env.new_global_ref(stream_class).unwrap();
                    let stream_ctor = env.get_method_id(stream_path, "<init>", "(J)V").unwrap();

                    let void_class = env.find_class(void_path).unwrap();
                    let void_class: GlobalRef = env.new_global_ref(void_class).unwrap();
                    let void_ctor = env.get_method_id(void_path, "<init>", "()V").unwrap();
                    let jlong_class = env.find_class(jlong_path).unwrap();
                    let jlong_class: GlobalRef = env.new_global_ref(jlong_class).unwrap();
                    let jlong_ctor = env.get_method_id(jlong_path, "<init>", "(J)V").unwrap();
                    let future_complete_method = env
                        .get_method_id(completable_future_path, "complete", "(Ljava/lang/Object;)Z")
                        .unwrap();
                    unsafe { STREAM_CLASS_CACHE.set(stream_class).unwrap() };
                    unsafe { STREAM_CTOR_CACHE.set(stream_ctor).unwrap() };
                    unsafe { VOID_CLASS_CACHE.set(void_class).unwrap() };
                    unsafe { VOID_CTOR_CACHE.set(void_ctor).unwrap() };
                    unsafe { JLONG_CLASS_CACHE.set(jlong_class).unwrap() };
                    unsafe { JLONG_CTOR_CACHE.set(jlong_ctor).unwrap() };
                    unsafe { FUTURE_COMPLETE_CACHE.set(future_complete_method).unwrap() };
                } else {
                    error!("Failed to attach current thread as daemon");
                }
            });
            JAVA_VM.with(|cell| {
                *cell.borrow_mut() = Some(java_vm.clone());
            });
            tokio_uring::start(async move {
                trace!("JNI tokio-uring runtime started");
                loop {
                    match rx.recv().await {
                        Some(cmd) => {
                            trace!("JNI tokio-uring receive command");
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
pub extern "system" fn JNI_OnUnload(_: JavaVM, _: *mut c_void) {
    unsafe { TX.take() };
}

// Frontend

#[no_mangle]
pub extern "system" fn Java_com_automq_elasticstream_client_jni_Frontend_getFrontend(
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
        match unsafe { TX.get() } {
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

/// # Safety
///
/// Expose `C` API to Java
#[no_mangle]
pub unsafe extern "system" fn Java_com_automq_elasticstream_client_jni_Frontend_freeFrontend(
    mut _env: JNIEnv,
    _class: JClass,
    ptr: *mut Frontend,
) {
    // Take ownership of the pointer by wrapping it with a Box
    let _ = unsafe { Box::from_raw(ptr) };
}

/// # Safety
///
/// Expose `C` API to Java
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
        let front_end = unsafe { &mut *ptr };
        Command::CreateStream {
            front_end,
            replica: replica as u8,
            ack_count: ack as u8,
            retention: Duration::from_millis(retention_millis as u64),
            future,
        }
    });
    if let Ok(command) = command {
        if let Some(tx) = unsafe { TX.get() } {
            if let Err(_e) = tx.send(command) {
                error!("Failed to dispatch CreateStream command to tokio-uring runtime");
                throw_exception(
                    &mut env,
                    "Failed to dispatch CreateStream command to tokio-uring runtime",
                );
            }
        } else {
            info!("JNI command channel was dropped. Ignore a CreateStream request");
            throw_exception(
                &mut env,
                "JNI command channel was dropped. Ignore a CreateStream request",
            );
        }
    } else {
        info!("Failed to construct CreateStream command");
        throw_exception(&mut env, "Failed to construct CreateStream command");
    };
}

/// # Safety
///
/// Expose `C` API to Java
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
        let front_end = unsafe { &mut *ptr };
        Command::OpenStream {
            front_end,
            stream_id: id as u64,
            epoch: epoch as u64,
            future,
        }
    });
    if let Ok(command) = command {
        if let Some(tx) = unsafe { TX.get() } {
            if let Err(_e) = tx.send(command) {
                error!("Failed to dispatch OpenStream command to tokio-uring runtime");
                throw_exception(
                    &mut env,
                    "Failed to dispatch OpenStream command to tokio-uring runtime",
                );
            }
        } else {
            info!("JNI command channel was dropped. Ignore an OpenStream request");
            throw_exception(
                &mut env,
                "JNI command channel was dropped. Ignore an OpenStream request",
            );
        }
    } else {
        info!("Failed to construct OpenStream command");
        throw_exception(&mut env, "Failed to construct OpenStream command");
    }
}

/// # Safety
///
/// Expose `C` API to Java
#[no_mangle]
pub unsafe extern "system" fn Java_com_automq_elasticstream_client_jni_Stream_freeStream(
    _env: JNIEnv,
    _class: JClass,
    ptr: *mut Stream,
) {
    // Take ownership of the pointer by wrapping it with a Box
    let _ = Box::from_raw(ptr);
}

/// # Safety
///
/// Expose `C` API to Java
#[no_mangle]
pub unsafe extern "system" fn Java_com_automq_elasticstream_client_jni_Stream_asyncClose(
    mut env: JNIEnv,
    _class: JClass,
    ptr: *mut Stream,
    future: JObject,
) {
    trace!("Started jni_Stream_asyncClose");
    let command = env.new_global_ref(future).map(|future| {
        let stream = unsafe { &mut *ptr };
        Command::CloseStream { stream, future }
    });
    if let Ok(command) = command {
        if let Some(tx) = unsafe { TX.get() } {
            if let Err(_e) = tx.send(command) {
                error!("Failed to dispatch CloseStream command to tokio-uring runtime");
                throw_exception(
                    &mut env,
                    "Failed to dispatch CloseStream command to tokio-uring runtime",
                );
            } else {
                trace!("Dispatched the CloseStream command to tokio-uring runtime");
            }
        } else {
            info!("JNI command channel was dropped. Ignore a CloseStream request");
            throw_exception(
                &mut env,
                "JNI command channel was dropped. Ignore a CloseStream request",
            );
        }
    } else {
        info!("Failed to construct CloseStream command.");
        throw_exception(&mut env, "Failed to construct CloseStream command.");
    }
}

/// # Safety
///
/// Expose `C` API to Java
#[no_mangle]
pub unsafe extern "system" fn Java_com_automq_elasticstream_client_jni_Stream_startOffset(
    mut env: JNIEnv,
    _class: JClass,
    ptr: *mut Stream,
    future: JObject,
) {
    trace!("Started jni_Stream_startOffset");
    let command = env.new_global_ref(future).map(|future| {
        let stream = unsafe { &mut *ptr };
        Command::StartOffset { stream, future }
    });
    if let Ok(command) = command {
        if let Some(tx) = unsafe { TX.get() } {
            if let Err(_e) = tx.send(command) {
                error!("Failed to dispatch StartOffset command to tokio-uring runtime");
                throw_exception(
                    &mut env,
                    "Failed to dispatch StartOffset command to tokio-uring runtime",
                );
            } else {
                trace!("Dispatched the StartOffset command to tokio-uring runtime");
            }
        } else {
            info!("JNI command channel was dropped. Ignore a StartOffset request");
            throw_exception(
                &mut env,
                "JNI command channel was dropped. Ignore a StartOffset request",
            );
        }
    } else {
        info!("Failed to construct StartOffset command.");
        throw_exception(&mut env, "Failed to construct StartOffset command.");
    }
}

/// # Safety
///
/// Expose `C` API to Java
#[no_mangle]
pub unsafe extern "system" fn Java_com_automq_elasticstream_client_jni_Stream_nextOffset(
    mut env: JNIEnv,
    _class: JClass,
    ptr: *mut Stream,
    future: JObject,
) {
    trace!("Started jni_Stream_nextOffset");
    let command = env.new_global_ref(future).map(|future| {
        let stream = unsafe { &mut *ptr };
        Command::NextOffset { stream, future }
    });
    if let Ok(command) = command {
        if let Some(tx) = unsafe { TX.get() } {
            if let Err(_e) = tx.send(command) {
                error!("Failed to dispatch NextOffset command to tokio-uring runtime");
                throw_exception(
                    &mut env,
                    "Failed to dispatch NextOffset command to tokio-uring runtime",
                );
            } else {
                trace!("Dispatched the NextOffset command to tokio-uring runtime");
            }
        } else {
            info!("JNI command channel was dropped. Ignore a NextOffset request");
            throw_exception(
                &mut env,
                "JNI command channel was dropped. Ignore a NextOffset request",
            );
        }
    } else {
        info!("Failed to construct NextOffset command");
        throw_exception(&mut env, "Failed to construct NextOffset command");
    }
}

/// # Safety
///
/// Expose `C` API to Java
#[no_mangle]
pub unsafe extern "system" fn Java_com_automq_elasticstream_client_jni_Stream_append(
    mut env: JNIEnv,
    _class: JClass,
    ptr: *mut Stream,
    data: JObject,
    future: JObject,
) {
    trace!("Started jni_Stream_append");
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
                throw_exception(
                    &mut env,
                    "Failed to dispatch Append command to tokio-uring runtime",
                );
            } else {
                trace!("Dispatched the Append command to tokio-uring runtime");
            }
        } else {
            info!("JNI command channel was dropped. Ignore an Append request");
            throw_exception(
                &mut env,
                "JNI command channel was dropped. Ignore an Append request",
            );
        }
    } else {
        info!("Failed to construct Append command");
        throw_exception(&mut env, "Failed to construct Append command");
    }
    trace!("Finished jni_Stream_append");
}

/// # Safety
///
/// Expose `C` API to Java
#[no_mangle]
pub unsafe extern "system" fn Java_com_automq_elasticstream_client_jni_Stream_read(
    mut env: JNIEnv,
    _class: JClass,
    ptr: *mut Stream,
    start_offset: jlong,
    end_offset: jlong,
    batch_max_bytes: jint,
    future: JObject,
) {
    trace!("Started jni_Stream_read");
    let command = env.new_global_ref(future).map(|future| {
        let stream = unsafe { &mut *ptr };
        Command::Read {
            stream,
            start_offset,
            end_offset,
            batch_max_bytes,
            future,
        }
    });
    if let Ok(command) = command {
        if let Some(tx) = unsafe { TX.get() } {
            if let Err(_e) = tx.send(command) {
                error!("Failed to dispatch Read command to tokio-uring runtime");
                throw_exception(
                    &mut env,
                    "Failed to dispatch Read command to tokio-uring runtime",
                );
            } else {
                trace!("Dispatched the Read command to tokio-uring runtime");
            }
        } else {
            info!("JNI command channel was dropped. Ignore a Read request");
            throw_exception(
                &mut env,
                "JNI command channel was dropped. Ignore a Read request",
            );
        }
    } else {
        info!("Failed to construct Read command");
        throw_exception(&mut env, "Failed to construct Read command");
    }
}

fn call_future_complete_method(mut env: JNIEnv, future: GlobalRef, obj: JObject) {
    let obj = env.auto_local(obj);
    let s = JValueGen::from(&obj);
    let _stopwatch = Stopwatch::new("Future#complete");
    let method = unsafe { FUTURE_COMPLETE_CACHE.get() }.unwrap();
    if unsafe {
        env.call_method_unchecked(
            future,
            method,
            jni::signature::ReturnType::Primitive(jni::signature::Primitive::Boolean),
            &[s.as_jni()],
        )
    }
    .is_err()
    {
        panic!("Failed to call future complete method");
    }
}
fn call_future_complete_exceptionally_method(
    env: &mut JNIEnv,
    future: GlobalRef,
    err: ClientError,
) {
    let class_path_pre = "com/automq/elasticstream/client/api/ElasticStreamClientException$";
    let ctor_sig_string = "(Ljava/lang/String;)V";
    let ctor_sig_long = "(J)V";
    let ctor_sig_void = "()V";
    let ctor_sig_int = "(I)V";
    let obj = match err {
        ClientError::ConnectionTimeout => {
            let exception_class = env.find_class(class_path_pre.to_owned() + "ConnectionTimeout");
            match exception_class {
                Ok(exception_class) => env.new_object(exception_class, ctor_sig_void, &[]),
                Err(err) => Err(err),
            }
        }
        ClientError::ConnectionReset(str) => {
            let exception_class = env.find_class(class_path_pre.to_owned() + "ConnectionReset");
            let str = env.new_string(str);
            match (exception_class, str) {
                (Ok(exception_class), Ok(str)) => env.new_object(
                    exception_class,
                    ctor_sig_string,
                    &[JValue::Object(str.as_ref())],
                ),
                (Err(err), _) => Err(err),
                (_, Err(err)) => Err(err),
            }
        }
        ClientError::StreamNotFound(stream_id) => {
            let exception_class = env.find_class(class_path_pre.to_owned() + "StreamNotFound");
            match exception_class {
                Ok(exception_class) => env.new_object(
                    exception_class,
                    ctor_sig_long,
                    &[jni::objects::JValueGen::Long(stream_id)],
                ),
                Err(err) => Err(err),
            }
        }
        ClientError::BrokenChannel(str) => {
            let exception_class = env.find_class(class_path_pre.to_owned() + "BrokenChannel");
            let str = env.new_string(str);
            match (exception_class, str) {
                (Ok(exception_class), Ok(str)) => env.new_object(
                    exception_class,
                    ctor_sig_string,
                    &[JValue::Object(str.as_ref())],
                ),
                (Err(err), _) => Err(err),
                (_, Err(err)) => Err(err),
            }
        }
        ClientError::RpcClientError(err) => {
            let class_path_pre =
                "com/automq/elasticstream/client/api/ElasticStreamClientException$RpcClientError";
            match err {
                client::error::ClientError::BadAddress => {
                    let exception_class = env.find_class(class_path_pre.to_owned() + "BadAddress");
                    match exception_class {
                        Ok(exception_class) => env.new_object(exception_class, ctor_sig_void, &[]),
                        Err(err) => Err(err),
                    }
                }
                client::error::ClientError::BadRequest => {
                    let exception_class = env.find_class(class_path_pre.to_owned() + "BadRequest");
                    match exception_class {
                        Ok(exception_class) => env.new_object(exception_class, ctor_sig_void, &[]),
                        Err(err) => Err(err),
                    }
                }
                client::error::ClientError::ConnectionRefused(str) => {
                    let exception_class =
                        env.find_class(class_path_pre.to_owned() + "ConnectionRefused");
                    let str = env.new_string(str);
                    match (exception_class, str) {
                        (Ok(exception_class), Ok(str)) => env.new_object(
                            exception_class,
                            ctor_sig_string,
                            &[JValue::Object(str.as_ref())],
                        ),
                        (Err(err), _) => Err(err),
                        (_, Err(err)) => Err(err),
                    }
                }
                client::error::ClientError::ConnectTimeout(str) => {
                    let exception_class =
                        env.find_class(class_path_pre.to_owned() + "ConnectTimeout");
                    let str = env.new_string(str);
                    match (exception_class, str) {
                        (Ok(exception_class), Ok(str)) => env.new_object(
                            exception_class,
                            ctor_sig_string,
                            &[JValue::Object(str.as_ref())],
                        ),
                        (Err(err), _) => Err(err),
                        (_, Err(err)) => Err(err),
                    }
                }
                client::error::ClientError::ConnectFailure(str) => {
                    let exception_class =
                        env.find_class(class_path_pre.to_owned() + "ConnectFailure");
                    let str = env.new_string(str);
                    match (exception_class, str) {
                        (Ok(exception_class), Ok(str)) => env.new_object(
                            exception_class,
                            ctor_sig_string,
                            &[JValue::Object(str.as_ref())],
                        ),
                        (Err(err), _) => Err(err),
                        (_, Err(err)) => Err(err),
                    }
                }
                client::error::ClientError::DisableNagleAlgorithm => {
                    let exception_class =
                        env.find_class(class_path_pre.to_owned() + "DisableNagleAlgorithm");
                    match exception_class {
                        Ok(exception_class) => env.new_object(exception_class, ctor_sig_void, &[]),
                        Err(err) => Err(err),
                    }
                }
                client::error::ClientError::ChannelClosing(str) => {
                    let exception_class =
                        env.find_class(class_path_pre.to_owned() + "ChannelClosing");
                    let str = env.new_string(str);
                    match (exception_class, str) {
                        (Ok(exception_class), Ok(str)) => env.new_object(
                            exception_class,
                            ctor_sig_string,
                            &[JValue::Object(str.as_ref())],
                        ),
                        (Err(err), _) => Err(err),
                        (_, Err(err)) => Err(err),
                    }
                }
                client::error::ClientError::Append(err) => {
                    let exception_class = env.find_class(class_path_pre.to_owned() + "Append");
                    match exception_class {
                        Ok(exception_class) => env.new_object(
                            exception_class,
                            ctor_sig_int,
                            &[jni::objects::JValueGen::Int(err.0 as i32)],
                        ),
                        Err(err) => Err(err),
                    }
                }
                client::error::ClientError::CreateRange(err) => {
                    let exception_class = env.find_class(class_path_pre.to_owned() + "CreateRange");
                    match exception_class {
                        Ok(exception_class) => env.new_object(
                            exception_class,
                            ctor_sig_int,
                            &[jni::objects::JValueGen::Int(err.0 as i32)],
                        ),
                        Err(err) => Err(err),
                    }
                }
                client::error::ClientError::ServerInternal => {
                    let exception_class =
                        env.find_class(class_path_pre.to_owned() + "ServerInternal");
                    match exception_class {
                        Ok(exception_class) => env.new_object(exception_class, ctor_sig_void, &[]),
                        Err(err) => Err(err),
                    }
                }
                client::error::ClientError::ClientInternal => {
                    let exception_class =
                        env.find_class(class_path_pre.to_owned() + "ClientInternal");
                    match exception_class {
                        Ok(exception_class) => env.new_object(exception_class, ctor_sig_void, &[]),
                        Err(err) => Err(err),
                    }
                }
                client::error::ClientError::RpcTimeout { timeout } => {
                    let exception_class = env.find_class(class_path_pre.to_owned() + "RpcTimeout");
                    match exception_class {
                        Ok(exception_class) => env.new_object(
                            exception_class,
                            ctor_sig_long,
                            &[jni::objects::JValueGen::Long(timeout.as_millis() as i64)],
                        ),
                        Err(err) => Err(err),
                    }
                }
            }
        }
        ClientError::Replication(err) => {
            let class_path_pre =
                "com/automq/elasticstream/client/api/ElasticStreamClientException$Replication";
            match err {
                replication::ReplicationError::RpcTimeout => {
                    let exception_class = env.find_class(class_path_pre.to_owned() + "RpcTimeout");
                    match exception_class {
                        Ok(exception_class) => env.new_object(exception_class, ctor_sig_void, &[]),
                        Err(err) => Err(err),
                    }
                }
                replication::ReplicationError::Internal => {
                    let exception_class = env.find_class(class_path_pre.to_owned() + "Internal");
                    match exception_class {
                        Ok(exception_class) => env.new_object(exception_class, ctor_sig_void, &[]),
                        Err(err) => Err(err),
                    }
                }
                replication::ReplicationError::AlreadySealed => {
                    let exception_class =
                        env.find_class(class_path_pre.to_owned() + "AlreadySealed");
                    match exception_class {
                        Ok(exception_class) => env.new_object(exception_class, ctor_sig_void, &[]),
                        Err(err) => Err(err),
                    }
                }
                replication::ReplicationError::PreconditionRequired => {
                    let exception_class =
                        env.find_class(class_path_pre.to_owned() + "PreconditionRequired");
                    match exception_class {
                        Ok(exception_class) => env.new_object(exception_class, ctor_sig_void, &[]),
                        Err(err) => Err(err),
                    }
                }
                replication::ReplicationError::AlreadyClosed => {
                    let exception_class =
                        env.find_class(class_path_pre.to_owned() + "AlreadyClosed");
                    match exception_class {
                        Ok(exception_class) => env.new_object(exception_class, ctor_sig_void, &[]),
                        Err(err) => Err(err),
                    }
                }
                replication::ReplicationError::SealReplicaNotEnough => {
                    let exception_class =
                        env.find_class(class_path_pre.to_owned() + "SealReplicaNotEnough");
                    match exception_class {
                        Ok(exception_class) => env.new_object(exception_class, ctor_sig_void, &[]),
                        Err(err) => Err(err),
                    }
                }
                replication::ReplicationError::FetchOutOfRange => {
                    let exception_class =
                        env.find_class(class_path_pre.to_owned() + "FetchOutOfRange");
                    match exception_class {
                        Ok(exception_class) => env.new_object(exception_class, ctor_sig_void, &[]),
                        Err(err) => Err(err),
                    }
                }
                replication::ReplicationError::StreamNotExist => {
                    let exception_class =
                        env.find_class(class_path_pre.to_owned() + "StreamNotExist");
                    match exception_class {
                        Ok(exception_class) => env.new_object(exception_class, ctor_sig_void, &[]),
                        Err(err) => Err(err),
                    }
                }
            }
        }
        ClientError::Internal(str) => {
            let exception_class = env.find_class(class_path_pre.to_owned() + "Internal");
            let str = env.new_string(str);
            match (exception_class, str) {
                (Ok(exception_class), Ok(str)) => env.new_object(
                    exception_class,
                    ctor_sig_string,
                    &[JValue::Object(str.as_ref())],
                ),
                (Err(err), _) => Err(err),
                (_, Err(err)) => Err(err),
            }
        }
    };

    if let Ok(obj) = obj {
        let s = JValueGen::from(obj);
        let _stopwatch = Stopwatch::new("Future#completeExceptionally");
        if env
            .call_method(
                future,
                "completeExceptionally",
                "(Ljava/lang/Throwable;)Z",
                &[s.borrow()],
            )
            .is_err()
        {
            panic!("Failed to call future completeExceptionally method");
        }
    } else {
        panic!("Failed to create exception object");
    }
}

fn get_thread_local_jenv(cell: &RefCell<Option<*mut *const JNINativeInterface_>>) -> JNIEnv {
    let env_ptr = cell
        .borrow()
        .expect("Couldn't get raw ptr of thread local jenv");
    unsafe { JNIEnv::from_raw(env_ptr).expect("Couldn't create a JNIEnv from raw pointer") }
}

fn throw_exception(env: &mut JNIEnv, msg: &str) {
    let exception_path =
        "com/automq/elasticstream/client/api/ElasticStreamClientException$Internal";
    let _ = env.exception_clear();
    if env.throw_new(exception_path, msg).is_err() {
        panic!("Failed to throw new exception");
    }
}

fn complete_future_with_stream(future: GlobalRef, ptr: i64) {
    JENV.with(|cell| {
        let mut env = get_thread_local_jenv(cell);
        let stream_class = unsafe { STREAM_CLASS_CACHE.get() };
        let stream_ctor = unsafe { STREAM_CTOR_CACHE.get() };
        if let (Some(stream_class), Some(stream_ctor)) = (stream_class, stream_ctor) {
            if let Ok(obj) = unsafe {
                env.new_object_unchecked(
                    stream_class,
                    *stream_ctor,
                    &[jni::objects::JValue::Long(ptr).as_jni()],
                )
            } {
                call_future_complete_method(env, future, obj);
            } else {
                panic!("Couldn't create Stream object");
            }
        } else {
            panic!("Couldn't find Stream class");
        }
    });
}

fn complete_future_with_jlong(future: GlobalRef, value: i64) {
    JENV.with(|cell| {
        let mut env = get_thread_local_jenv(cell);
        let long_class = unsafe { JLONG_CLASS_CACHE.get() };
        let long_ctor = unsafe { JLONG_CTOR_CACHE.get() };
        if let (Some(long_class), Some(long_ctor)) = (long_class, long_ctor) {
            if let Ok(obj) = unsafe {
                env.new_object_unchecked(
                    long_class,
                    *long_ctor,
                    &[jni::objects::JValue::Long(value).as_jni()],
                )
            } {
                call_future_complete_method(env, future, obj);
            } else {
                panic!("Failed to create Long object");
            }
        } else {
            panic!("Failed to find Long object");
        }
    });
}

fn complete_future_with_void(future: GlobalRef) {
    JENV.with(|cell| {
        let mut env = get_thread_local_jenv(cell);
        let void_class = unsafe { VOID_CLASS_CACHE.get() };
        let void_ctor = unsafe { VOID_CTOR_CACHE.get() };
        if let (Some(void_class), Some(void_ctor)) = (void_class, void_ctor) {
            if let Ok(obj) = unsafe { env.new_object_unchecked(void_class, *void_ctor, &[]) } {
                call_future_complete_method(env, future, obj);
            } else {
                panic!("Failed to create Void object");
            }
        } else {
            panic!("Failed to find Void class");
        }
    });
}

fn complete_future_with_error(future: GlobalRef, err: ClientError) {
    JENV.with(|cell| {
        let mut env = get_thread_local_jenv(cell);
        call_future_complete_exceptionally_method(&mut env, future, err);
    });
}
