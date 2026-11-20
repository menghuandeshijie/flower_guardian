//! 桌面自检 CLI：在没有 Android 设备时用它验证「tract 推理结果 == Ultralytics 结果」。
//!
//! 用法：
//! ```bash
//! cargo run --release --bin tract-detect -- \
//!   --model ../../android/app/src/main/assets/models/plant_yolo11n.onnx \
//!   --image ~/Pictures/monstera.jpg --classes 47 --conf 0.35 --iou 0.45
//! ```
//! 输出为 JSON，可直接与 Python 端 `results[0].boxes` 做 IoU / 类别对齐比对。

use std::path::PathBuf;

use tract_engine::YoloDetector;

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn parse_or<T: std::str::FromStr>(args: &[String], name: &str, default: T) -> T {
    arg(args, name)
        .and_then(|v| v.parse::<T>().ok())
        .unwrap_or(default)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(model) = arg(&args, "--model") else {
        eprintln!("缺少 --model <onnx 路径>");
        std::process::exit(2);
    };
    let Some(image) = arg(&args, "--image") else {
        eprintln!("缺少 --image <图片路径>");
        std::process::exit(2);
    };

    let imgsz: u32 = parse_or(&args, "--imgsz", 640);
    let classes: usize = parse_or(&args, "--classes", 0);
    let conf: f32 = parse_or(&args, "--conf", 0.35);
    let iou: f32 = parse_or(&args, "--iou", 0.45);

    let load_started = std::time::Instant::now();
    let detector = match YoloDetector::from_path(&PathBuf::from(&model), imgsz, classes) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("模型加载失败: {e}");
            std::process::exit(1);
        }
    };
    let load_ms = load_started.elapsed().as_secs_f64() * 1000.0;

    let started = std::time::Instant::now();
    let (detections, width, height) = match detector.detect_path(&PathBuf::from(&image), conf, iou) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("推理失败: {e}");
            std::process::exit(1);
        }
    };
    let infer_ms = started.elapsed().as_secs_f64() * 1000.0;

    let payload = serde_json::json!({
        "model": model,
        "image": image,
        "width": width,
        "height": height,
        "loadMs": load_ms,
        "inferMs": infer_ms,
        "detections": detections,
    });
    println!("{}", serde_json::to_string_pretty(&payload).unwrap());
}
