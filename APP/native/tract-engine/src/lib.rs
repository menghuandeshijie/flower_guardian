//! Flower Guardian 端侧推理引擎核心。
//!
//! 用 Rust 的 tract（ONNX 前端 + CPU 后端）执行 YOLO 检测，替代原先未落地的
//! `onnxruntime-react-native` 方案：单文件 .so、无额外 JNI/Java 包依赖、
//! 支持 aarch64/armv7/x86_64 交叉编译。

pub mod yolo;

#[cfg(target_os = "android")]
mod android;

use std::path::Path;

use anyhow::Result;
use image::RgbImage;
use tract_onnx::prelude::*;

pub use yolo::{Detection, LetterboxMeta};

type Plan = SimplePlan<TypedFact, Box<dyn TypedOp>, TypedModel>;

/// 一个已编译好的 YOLO 检测器（tract plan + 输入规格）
pub struct YoloDetector {
    plan: Plan,
    input_size: u32,
    num_classes: usize,
}

impl YoloDetector {
    /// 加载 ONNX 模型并做图优化。首次加载会做一次完整的 plan 构建（数百毫秒级），
    /// 之后 `detect_*` 只做前向计算。
    ///
    /// * `input_size`：导出时的 `imgsz`，必须与模型固定输入一致（本项目为 640）
    /// * `num_classes`：0 表示由输出通道数推断
    pub fn from_path(path: &Path, input_size: u32, num_classes: usize) -> Result<Self> {
        if !path.exists() {
            anyhow::bail!("模型文件不存在: {}", path.display());
        }
        let plan = tract_onnx::onnx()
            .model_for_path(path)?
            .into_optimized()?
            .into_runnable()?;
        Ok(Self {
            plan,
            input_size,
            num_classes,
        })
    }

    pub fn input_size(&self) -> u32 {
        self.input_size
    }

    /// 从文件推理，返回（检测框, 原图宽, 原图高）
    pub fn detect_path(
        &self,
        path: &Path,
        conf_threshold: f32,
        iou_threshold: f32,
    ) -> Result<(Vec<Detection>, u32, u32)> {
        let img = image::ImageReader::open(path)?
            .with_guessed_format()?
            .decode()?
            .to_rgb8();
        self.detect_rgb(&img, conf_threshold, iou_threshold)
    }

    /// 对已解码的 RGB 图像推理
    pub fn detect_rgb(
        &self,
        img: &RgbImage,
        conf_threshold: f32,
        iou_threshold: f32,
    ) -> Result<(Vec<Detection>, u32, u32)> {
        let (src_w, src_h) = img.dimensions();
        let lb = yolo::letterbox(img, self.input_size)?;

        let shape = vec![
            1usize,
            3,
            self.input_size as usize,
            self.input_size as usize,
        ];
        let input = Tensor::from_shape(&shape, &lb.data)?;
        let outputs = self.plan.run(tvec!(input.into()))?;

        let out = outputs
            .first()
            .ok_or_else(|| anyhow::anyhow!("模型没有任何输出"))?;
        let view = out.to_array_view::<f32>()?;
        let shape: Vec<usize> = view.shape().to_vec();

        let owned;
        let flat: &[f32] = match view.as_slice() {
            Some(s) => s,
            None => {
                owned = view.iter().copied().collect::<Vec<f32>>();
                &owned
            }
        };

        let dets = yolo::decode(
            flat,
            &shape,
            &lb.meta,
            self.num_classes,
            conf_threshold,
            iou_threshold,
        )?;
        Ok((dets, src_w, src_h))
    }
}

/// 便捷函数：加载 + 单张推理（CLI 与自检使用）
pub fn detect_once(
    model_path: &Path,
    image_path: &Path,
    input_size: u32,
    num_classes: usize,
    conf_threshold: f32,
    iou_threshold: f32,
) -> Result<(Vec<Detection>, u32, u32)> {
    let det = YoloDetector::from_path(model_path, input_size, num_classes)?;
    det.detect_path(image_path, conf_threshold, iou_threshold)
}
