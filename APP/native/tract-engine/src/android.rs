//! Android JNI 入口。
//!
//! 与 `TractEngineModule.kt` 中的 `external fun` 一一对应，命名规则：
//! `Java_com_flowerguardian_app_tract_TractEngineModule_<method>`
//!
//! 设计约定：
//! * `nativeCreate` 返回句柄 id（>0 成功，0 失败并抛 Java 异常）；
//! * `nativeDetect` 返回 JSON 字符串（`null` 表示失败，Kotlin 侧转成 reject）；
//! * 所有外部调用都套 `catch_unwind`，避免 Rust panic 直接 abort 整个 App。

use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use jni::objects::{JObject, JString};
use jni::sys::{jfloat, jint, jlong, jstring};
use jni::JNIEnv;

use crate::YoloDetector;

static ENGINES: OnceLock<Mutex<HashMap<i64, Arc<YoloDetector>>>> = OnceLock::new();
static NEXT_ID: AtomicI64 = AtomicI64::new(1);

fn engines() -> &'static Mutex<HashMap<i64, Arc<YoloDetector>>> {
    ENGINES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn throw(env: &mut JNIEnv, msg: &str) {
    let _ = env.throw_new("java/lang/IllegalStateException", msg);
}

fn read_jstring(env: &mut JNIEnv, s: &JString) -> Option<String> {
    env.get_string(s)
        .ok()
        .map(|v| v.to_string_lossy().into_owned())
}

/// 创建检测器，返回句柄
#[no_mangle]
pub extern "system" fn Java_com_flowerguardian_app_tract_TractEngineModule_nativeCreate<'local>(
    mut env: JNIEnv<'local>,
    _this: JObject<'local>,
    model_path: JString<'local>,
    input_size: jint,
    num_classes: jint,
) -> jlong {
    let Some(path) = read_jstring(&mut env, &model_path) else {
        throw(&mut env, "模型路径无法解析");
        return 0;
    };

    let result = catch_unwind(AssertUnwindSafe(|| {
        YoloDetector::from_path(
            Path::new(&path),
            input_size.max(32) as u32,
            num_classes.max(0) as usize,
        )
    }));

    match result {
        Ok(Ok(det)) => {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            if let Ok(mut map) = engines().lock() {
                map.insert(id, Arc::new(det));
                log::info!("tract 引擎已创建: id={}, model={}", id, path);
                id
            } else {
                throw(&mut env, "引擎注册表已损坏");
                0
            }
        }
        Ok(Err(e)) => {
            throw(&mut env, &format!("模型加载失败: {e}"));
            0
        }
        Err(_) => {
            throw(&mut env, "模型加载时发生内部错误");
            0
        }
    }
}

/// 对图片文件推理，返回 JSON：
/// `{"detections":[...],"width":W,"height":H,"elapsedMs":12.3}`
#[no_mangle]
pub extern "system" fn Java_com_flowerguardian_app_tract_TractEngineModule_nativeDetect<'local>(
    mut env: JNIEnv<'local>,
    _this: JObject<'local>,
    handle: jlong,
    image_path: JString<'local>,
    conf_threshold: jfloat,
    iou_threshold: jfloat,
) -> jstring {
    let Some(path) = read_jstring(&mut env, &image_path) else {
        throw(&mut env, "图片路径无法解析");
        return std::ptr::null_mut();
    };

    let engine = {
        let map = match engines().lock() {
            Ok(m) => m,
            Err(_) => {
                throw(&mut env, "引擎注册表已损坏");
                return std::ptr::null_mut();
            }
        };
        match map.get(&handle) {
            Some(e) => Arc::clone(e),
            None => {
                throw(&mut env, &format!("引擎句柄不存在: {handle}"));
                return std::ptr::null_mut();
            }
        }
    };

    let started = std::time::Instant::now();
    let result = catch_unwind(AssertUnwindSafe(|| {
        engine.detect_path(
            Path::new(&path),
            conf_threshold.clamp(0.0, 1.0),
            iou_threshold.clamp(0.0, 1.0),
        )
    }));

    let (detections, width, height) = match result {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            throw(&mut env, &format!("推理失败: {e}"));
            return std::ptr::null_mut();
        }
        Err(_) => {
            throw(&mut env, "推理时发生内部错误");
            return std::ptr::null_mut();
        }
    };

    let payload = serde_json::json!({
        "detections": detections,
        "width": width,
        "height": height,
        "elapsedMs": started.elapsed().as_secs_f64() * 1000.0,
    });

    match env.new_string(payload.to_string()) {
        Ok(s) => s.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// 释放句柄
#[no_mangle]
pub extern "system" fn Java_com_flowerguardian_app_tract_TractEngineModule_nativeDispose<'local>(
    _env: JNIEnv<'local>,
    _this: JObject<'local>,
    handle: jlong,
) {
    if let Ok(mut map) = engines().lock() {
        map.remove(&handle);
    }
}

/// 返回引擎版本，用于 JS 侧自检
#[no_mangle]
pub extern "system" fn Java_com_flowerguardian_app_tract_TractEngineModule_nativeVersion<'local>(
    env: JNIEnv<'local>,
    _this: JObject<'local>,
) -> jstring {
    let s = env
        .new_string(concat!("tract-engine ", env!("CARGO_PKG_VERSION")))
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut());
    s
}

/// 初始化 Android 日志（由 Kotlin 侧在加载库后调用一次）
#[no_mangle]
pub extern "system" fn Java_com_flowerguardian_app_tract_TractEngineModule_nativeInitLogging<'local>(
    _env: JNIEnv<'local>,
    _this: JObject<'local>,
) {
    let _ = android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("TractEngine"),
    );
}
