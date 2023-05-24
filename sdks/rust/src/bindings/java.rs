use jni::objects::{GlobalRef, JByteArray, JClass, JObject, JString, JValue, JValueGen};
use jni::sys::{jint, jlong, JNINativeInterface_, JNI_VERSION_1_8};
use jni::{JNIEnv, JavaVM};
use std::cell::{OnceCell, RefCell};
use std::ffi::c_void;
use std::io::IoSlice;
use std::slice::from_raw_parts;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;

use tokio::runtime::{Builder, Runtime};

use crate::{ClientError, Frontend, Stream, StreamOptions};

use super::cmd::Command;

static mut RUNTIME: OnceCell<Runtime> = OnceCell::new();
static mut TX: OnceCell<tokio::sync::mpsc::UnboundedSender<Command>> = OnceCell::new();

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
        Command::MinOffset { stream, future } => {
            process_min_offset_command(stream, future).await;
        }
        Command::MaxOffset { stream, future } => {
            process_max_offset_command(stream, future).await;
        }
        Command::Append {
            stream,
            slice,
            count,
            future,
        } => {
            process_append_command(stream, slice, count, future).await;
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
async fn process_append_command(stream: &mut Stream, slice: &[u8], count: u32, future: GlobalRef) {
    let result = stream
        .append(IoSlice::new(slice), count.try_into().unwrap())
        .await;
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
        Ok(result) => {
            let result: &[u8] = result.as_ref();
            JENV.with(|cell| {
                let env = unsafe { get_thread_local_jenv(cell) };
                let output = env.byte_array_from_slice(&result).unwrap();
                unsafe { call_future_complete_method(env, future, JObject::from(output)) };
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

async fn process_min_offset_command(stream: &mut Stream, future: GlobalRef) {
    let result = stream.min_offset().await;
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

async fn process_max_offset_command(stream: &mut Stream, future: GlobalRef) {
    let result = stream.max_offset().await;
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
    tx.send(result);
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
                let stream_class = env.find_class("sdk/elastic/stream/jni/Stream").unwrap();
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
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
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
    if let Some(runtime) = RUNTIME.take() {
        runtime.shutdown_background();
    }
}

// Frontend

#[no_mangle]
pub unsafe extern "system" fn Java_sdk_elastic_stream_jni_Frontend_getFrontend(
    mut env: JNIEnv,
    _class: JClass,
    access_point: JString,
) -> jlong {
    let (tx, rx) = oneshot::channel();
    let access_point: String = env.get_string(&access_point).unwrap().into();
    let command = Command::GetFrontend {
        access_point: access_point,
        tx: tx,
    };
    let _ = TX.get().unwrap().send(command);
    let result = rx.blocking_recv().unwrap();
    match result {
        Ok(frontend) => Box::into_raw(Box::new(frontend)) as jlong,
        Err(err) => {
            let _ = env.exception_clear();
            env.throw_new("java/lang/Exception", err.to_string())
                .expect("Couldn't throw exception");
            0
        }
    }
}

#[no_mangle]
pub unsafe extern "system" fn Java_sdk_elastic_stream_jni_Frontend_freeFrontend(
    mut _env: JNIEnv,
    _class: JClass,
    ptr: *mut Frontend,
) {
    // Take ownership of the pointer by wrapping it with a Box
    let _ = Box::from_raw(ptr);
}

#[no_mangle]
pub unsafe extern "system" fn Java_sdk_elastic_stream_jni_Frontend_create(
    env: JNIEnv,
    _class: JClass,
    ptr: *mut Frontend,
    replica: jint,
    ack: jint,
    retention_millis: jlong,
    future: JObject,
) {
    let front_end = &mut *ptr;
    let future = env.new_global_ref(future).unwrap();
    let command = Command::CreateStream {
        front_end: front_end,
        replica: replica as u8,
        ack_count: ack as u8,
        retention: Duration::from_millis(retention_millis as u64),
        future,
    };
    let _ = TX.get().unwrap().send(command);
}
#[no_mangle]
pub unsafe extern "system" fn Java_sdk_elastic_stream_jni_Frontend_open(
    env: JNIEnv,
    _class: JClass,
    ptr: *mut Frontend,
    id: jlong,
    future: JObject,
) {
    let front_end = &mut *ptr;
    let future = env.new_global_ref(future).unwrap();
    debug_assert!(id >= 0, "Stream ID should be non-negative");
    let command = Command::OpenStream {
        front_end,
        stream_id: id as u64,
        epoch: 0,
        future: future,
    };
    let _ = TX.get().unwrap().send(command);
}

// Stream

#[no_mangle]
pub unsafe extern "system" fn Java_sdk_elastic_stream_jni_Stream_freeStream(
    env: JNIEnv,
    _class: JClass,
    ptr: *mut Stream,
) {
    // Take ownership of the pointer by wrapping it with a Box
    let _ = Box::from_raw(ptr);
}

#[no_mangle]
pub unsafe extern "system" fn Java_sdk_elastic_stream_jni_Stream_minOffset(
    mut env: JNIEnv,
    _class: JClass,
    ptr: *mut Stream,
    future: JObject,
) {
    let stream = &mut *ptr;
    let future = env.new_global_ref(future).unwrap();
    let command = Command::MinOffset {
        stream: stream,
        future: future,
    };
    let _ = TX.get().unwrap().send(command);
}

#[no_mangle]
pub unsafe extern "system" fn Java_sdk_elastic_stream_jni_Stream_maxOffset(
    mut env: JNIEnv,
    _class: JClass,
    ptr: *mut Stream,
    future: JObject,
) {
    let stream = &mut *ptr;
    let future = env.new_global_ref(future).unwrap();
    let command = Command::MaxOffset {
        stream: stream,
        future: future,
    };
    let _ = TX.get().unwrap().send(command);
}

#[no_mangle]
pub unsafe extern "system" fn Java_sdk_elastic_stream_jni_Stream_append(
    mut env: JNIEnv,
    _class: JClass,
    ptr: *mut Stream,
    mut data: JByteArray,
    count: jint,
    future: JObject,
) {
    let stream = &mut *ptr;
    let future = env.new_global_ref(future).unwrap();
    let array = env
        .get_array_elements(&data, jni::objects::ReleaseMode::CopyBack)
        .unwrap();
    let len = env.get_array_length(&data).unwrap();
    let slice = from_raw_parts(array.as_ptr() as *mut u8, len.try_into().unwrap());

    let command = Command::Append {
        stream: stream,
        slice: slice,
        count: count as u32,
        future: future,
    };
    let _ = TX.get().unwrap().send(command);
}

#[no_mangle]
pub unsafe extern "system" fn Java_sdk_elastic_stream_jni_Stream_read(
    mut env: JNIEnv,
    _class: JClass,
    ptr: *mut Stream,
    start_offset: jlong,
    end_offset: jlong,
    batch_max_bytes: jint,
    future: JObject,
) {
    let stream = &mut *ptr;
    let future = env.new_global_ref(future).unwrap();
    let command = Command::Read {
        stream: stream,
        start_offset: start_offset,
        end_offset: end_offset,
        batch_max_bytes: batch_max_bytes,
        future: future,
    };
    let _ = TX.get().unwrap().send(command);
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
