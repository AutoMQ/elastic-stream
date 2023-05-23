use jni::objects::{GlobalRef, JByteArray, JClass, JObject, JString, JValue, JValueGen};
use jni::sys::{jint, jlong, JNINativeInterface_, JNI_VERSION_1_8};
use jni::{JNIEnv, JavaVM};
use std::cell::{OnceCell, RefCell};
use std::ffi::c_void;
use std::io::IoSlice;
use std::slice::from_raw_parts;
use std::sync::Arc;
use std::time::Duration;

use tokio::runtime::{Builder, Runtime};

use crate::{Frontend, Stream, StreamOptions};

static mut RUNTIME: OnceCell<Runtime> = OnceCell::new();

thread_local! {
    static JAVA_VM: RefCell<Option<Arc<JavaVM>>> = RefCell::new(None);
    static JENV: RefCell<Option<*mut jni::sys::JNIEnv>> = RefCell::new(None);
}
/// # Safety
///
/// This function could be only called by java vm when onload this lib.
#[no_mangle]
pub unsafe extern "system" fn JNI_OnLoad(vm: JavaVM, _: *mut c_void) -> jint {
    // TODO: make this configurable in the future
    let thread_count = num_cpus::get();
    let java_vm = Arc::new(vm);
    let runtime = Builder::new_multi_thread()
        .worker_threads(thread_count)
        .on_thread_start(move || {
            JENV.with(|cell| {
                let env = java_vm.attach_current_thread_as_daemon().unwrap();
                *cell.borrow_mut() = Some(env.get_raw());
            });
            JAVA_VM.with(|cell| {
                *cell.borrow_mut() = Some(java_vm.clone());
            });
        })
        .on_thread_stop(move || {
            JENV.with(|cell| {
                *cell.borrow_mut() = None;
            });
            JAVA_VM.with(|cell| unsafe {
                if let Some(vm) = cell.borrow_mut().take() {
                    vm.detach_current_thread();
                }
            });
        })
        .enable_time()
        .build()
        .unwrap();
    RUNTIME.set(runtime).unwrap();
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
    let access_point: String = env.get_string(&access_point).unwrap().into();
    let result = Frontend::new(&access_point);
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
    let options = StreamOptions {
        replica: replica.try_into().unwrap(),
        ack: ack.try_into().unwrap(),
        retention: Duration::from_millis(retention_millis.try_into().unwrap()),
    };
    let future = env.new_global_ref(future).unwrap();
    RUNTIME.get().unwrap().spawn(async move {
        let result = front_end.create(options).await;
        match result {
            Ok(stream) => {
                let ptr = Box::into_raw(Box::new(stream)) as jlong;
                JENV.with(|cell| {
                    let mut env = get_thread_local_jenv(cell);
                    let stream_class = env.find_class("sdk/elastic/stream/jni/Stream").unwrap();
                    let obj = env
                        .new_object(stream_class, "(J)V", &[jni::objects::JValueGen::Long(ptr)])
                        .unwrap();
                    call_future_complete_method(env, future, obj);
                });
            }
            Err(err) => {
                JENV.with(|cell| {
                    let mut env = get_thread_local_jenv(cell);
                    call_future_complete_exceptionally_method(&mut env, future, err.to_string());
                });
            }
        };
    });
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
    RUNTIME.get().unwrap().spawn(async move {
        debug_assert!(id >= 0, "Stream ID should be non-negative");
        let result = front_end.open(id as u64, 0).await;
        match result {
            Ok(stream) => {
                let ptr = Box::into_raw(Box::new(stream)) as jlong;
                JENV.with(|cell| {
                    let mut env = get_thread_local_jenv(cell);
                    let stream_class = env.find_class("sdk/elastic/stream/jni/Stream").unwrap();
                    let obj = env
                        .new_object(stream_class, "(J)V", &[jni::objects::JValueGen::Long(ptr)])
                        .unwrap();
                    call_future_complete_method(env, future, obj);
                });
            }
            Err(err) => {
                JENV.with(|cell| {
                    let mut env = get_thread_local_jenv(cell);
                    call_future_complete_exceptionally_method(&mut env, future, err.to_string());
                });
            }
        };
    });
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
    RUNTIME.get().unwrap().spawn(async move {
        let result = stream.min_offset().await;
        match result {
            Ok(offset) => {
                JENV.with(|cell| {
                    let mut env = get_thread_local_jenv(cell);

                    let long_class = env.find_class("java/lang/Long").unwrap();
                    let obj = env
                        .new_object(long_class, "(J)V", &[jni::objects::JValueGen::Long(offset)])
                        .unwrap();
                    call_future_complete_method(env, future, obj);
                });
            }
            Err(err) => {
                JENV.with(|cell| {
                    let mut env = get_thread_local_jenv(cell);
                    call_future_complete_exceptionally_method(&mut env, future, err.to_string());
                });
            }
        };
    });
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
    RUNTIME.get().unwrap().spawn(async move {
        let result = stream.max_offset().await;
        match result {
            Ok(offset) => {
                JENV.with(|cell| {
                    let mut env = get_thread_local_jenv(cell);
                    let long_class = env.find_class("java/lang/Long").unwrap();
                    let obj = env
                        .new_object(long_class, "(J)V", &[jni::objects::JValueGen::Long(offset)])
                        .unwrap();
                    call_future_complete_method(env, future, obj);
                });
            }
            Err(err) => {
                JENV.with(|cell| {
                    let mut env = get_thread_local_jenv(cell);
                    call_future_complete_exceptionally_method(&mut env, future, err.to_string());
                });
            }
        };
    });
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
    RUNTIME.get().unwrap().spawn(async move {
        let result = stream
            .append(IoSlice::new(slice), count.try_into().unwrap())
            .await;
        match result {
            Ok(result) => {
                let base_offset = result.base_offset;
                JENV.with(|cell| {
                    let mut env = get_thread_local_jenv(cell);
                    let long_class = env.find_class("java/lang/Long").unwrap();
                    let obj = env
                        .new_object(
                            long_class,
                            "(J)V",
                            &[jni::objects::JValueGen::Long(base_offset)],
                        )
                        .unwrap();
                    call_future_complete_method(env, future, obj);
                });
            }
            Err(err) => {
                JENV.with(|cell| {
                    let mut env = get_thread_local_jenv(cell);
                    call_future_complete_exceptionally_method(&mut env, future, err.to_string());
                });
            }
        };
    });
}

#[no_mangle]
pub unsafe extern "system" fn Java_sdk_elastic_stream_jni_Stream_read(
    mut env: JNIEnv,
    _class: JClass,
    ptr: *mut Stream,
    offset: jlong,
    limit: jint,
    max_bytes: jint,
    future: JObject,
) {
    let stream = &mut *ptr;
    let future = env.new_global_ref(future).unwrap();
    RUNTIME.get().unwrap().spawn(async move {
        let result = stream.read(offset, limit, max_bytes).await;
        match result {
            Ok(result) => {
                let result: &[u8] = result.as_ref();
                JENV.with(|cell| {
                    let env = get_thread_local_jenv(cell);
                    let output = env.byte_array_from_slice(&result).unwrap();
                    call_future_complete_method(env, future, JObject::from(output));
                });
            }
            Err(err) => {
                JENV.with(|cell| {
                    let mut env = get_thread_local_jenv(cell);
                    call_future_complete_exceptionally_method(&mut env, future, err.to_string());
                });
            }
        };
    });
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
