//! YOLO 预处理 / 后处理。
//!
//! 与 Ultralytics 的 Python 推理流程严格对齐：
//! 1. letterbox 到固定输入尺寸，填充色 114（RGB 三通道同值）；
//! 2. BGR->RGB 不做转换（`image` crate 解出的就是 RGB），归一化 /255，布局 NCHW；
//! 3. 输出张量按 Ultralytics 的 (1, 4+nc, N) 布局解码，同时兼容 (1, N, 4+nc)
//!    与端到端已 NMS 的 (N, 6) 布局；
//! 4. 置信度过滤 + 逐类贪心 NMS + 反 letterbox 映射回原图坐标。

use anyhow::{bail, Result};
use image::{imageops::FilterType, RgbImage};

/// Ultralytics letterbox 的默认填充值
pub const PAD_VALUE: u8 = 114;

/// 一个检测结果，坐标为原图像素坐标（左上/右下）
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detection {
    pub class_id: usize,
    pub score: f32,
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

/// letterbox 之后与坐标还原相关的一组元数据
#[derive(Debug, Clone, Copy)]
pub struct LetterboxMeta {
    pub ratio: f32,
    pub pad_w: f32,
    pub pad_h: f32,
    pub src_w: u32,
    pub src_h: u32,
    pub input_size: u32,
}

/// letterbox 结果：CHW 浮点张量 + 还原元数据
pub struct Letterboxed {
    pub data: Vec<f32>,
    pub meta: LetterboxMeta,
}

/// 等比缩放 + 居中填充到 `size x size`，输出 NCHW / 0~1
pub fn letterbox(img: &RgbImage, size: u32) -> Result<Letterboxed> {
    let (src_w, src_h) = img.dimensions();
    if src_w == 0 || src_h == 0 {
        bail!("图片尺寸非法: {}x{}", src_w, src_h);
    }
    if size == 0 {
        bail!("模型输入尺寸非法: 0");
    }

    let ratio = (size as f32 / src_w as f32).min(size as f32 / src_h as f32);
    let new_w = ((src_w as f32 * ratio).round() as u32).clamp(1, size);
    let new_h = ((src_h as f32 * ratio).round() as u32).clamp(1, size);
    let pad_w = (size - new_w) / 2;
    let pad_h = (size - new_h) / 2;

    let resized = image::imageops::resize(img, new_w, new_h, FilterType::Triangle);
    let mut canvas = RgbImage::from_pixel(size, size, image::Rgb([PAD_VALUE; 3]));
    image::imageops::overlay(&mut canvas, &resized, pad_w as i64, pad_h as i64);

    let plane = (size as usize) * (size as usize);
    let mut data = vec![0f32; 3 * plane];
    for (x, y, px) in canvas.enumerate_pixels() {
        let i = (y as usize) * (size as usize) + (x as usize);
        data[i] = px[0] as f32 / 255.0;
        data[plane + i] = px[1] as f32 / 255.0;
        data[2 * plane + i] = px[2] as f32 / 255.0;
    }

    Ok(Letterboxed {
        data,
        meta: LetterboxMeta {
            ratio,
            pad_w: pad_w as f32,
            pad_h: pad_h as f32,
            src_w,
            src_h,
            input_size: size,
        },
    })
}

/// row-major 连续张量的偏移计算
fn offset(shape: &[usize], idx: &[usize]) -> usize {
    let mut o = 0usize;
    let mut stride = 1usize;
    for k in (0..shape.len()).rev() {
        o += idx[k] * stride;
        stride *= shape[k];
    }
    o
}

/// 解码模型输出为原图坐标系下的检测框。
///
/// * `flat` / `shape`：模型输出张量（连续内存）
/// * `num_classes_hint`：传 0 时由输出通道数推断（4+nc）
pub fn decode(
    flat: &[f32],
    shape: &[usize],
    meta: &LetterboxMeta,
    num_classes_hint: usize,
    conf_threshold: f32,
    iou_threshold: f32,
) -> Result<Vec<Detection>> {
    let mut raw: Vec<Detection> = Vec::new();

    match shape.len() {
        // 端到端导出（已含 NMS）：(N, 6) => x1,y1,x2,y2,score,cls
        2 => {
            let (n, c) = (shape[0], shape[1]);
            if c < 6 {
                bail!("不支持的输出形状 {:?}", shape);
            }
            for i in 0..n {
                let score = flat[offset(shape, &[i, 4])];
                if score < conf_threshold {
                    continue;
                }
                let class_id = flat[offset(shape, &[i, 5])].max(0.0) as usize;
                raw.push(Detection {
                    class_id,
                    score,
                    x1: flat[offset(shape, &[i, 0])],
                    y1: flat[offset(shape, &[i, 1])],
                    x2: flat[offset(shape, &[i, 2])],
                    y2: flat[offset(shape, &[i, 3])],
                });
            }
        }
        // 常规导出：(1, 4+nc, N) 或 (1, N, 4+nc)
        3 => {
            let (d1, d2) = (shape[1], shape[2]);
            // 判定哪个维度是 (4+nc) 属性维：
            // 1) 已知类别数时优先精确匹配 4+nc；
            // 2) 否则属性维一定远小于候选框维（84 < 8400）。
            let expected_attrs = if num_classes_hint > 0 {
                Some(4 + num_classes_hint)
            } else {
                None
            };
            let (transposed, attrs, n) = match expected_attrs {
                Some(e) if d1 == e => (true, d1, d2),
                Some(e) if d2 == e => (false, d2, d1),
                _ if d1 <= d2 => (true, d1, d2),
                _ => (false, d2, d1),
            };

            let num_classes = if num_classes_hint > 0 {
                num_classes_hint
            } else {
                attrs.saturating_sub(4)
            };
            if attrs < 4 + num_classes {
                bail!(
                    "输出通道 {} 不足以容纳 4+{} 个类别（shape {:?}）",
                    attrs,
                    num_classes,
                    shape
                );
            }

            let get = |a: usize, i: usize| -> f32 {
                if transposed {
                    flat[offset(shape, &[0, a, i])]
                } else {
                    flat[offset(shape, &[0, i, a])]
                }
            };

            for i in 0..n {
                let mut best_class = 0usize;
                let mut best_score = 0f32;
                for c in 0..num_classes {
                    let s = get(4 + c, i);
                    if s > best_score {
                        best_score = s;
                        best_class = c;
                    }
                }
                if best_score < conf_threshold {
                    continue;
                }
                let (cx, cy, w, h) = (get(0, i), get(1, i), get(2, i), get(3, i));
                raw.push(Detection {
                    class_id: best_class,
                    score: best_score,
                    x1: cx - w / 2.0,
                    y1: cy - h / 2.0,
                    x2: cx + w / 2.0,
                    y2: cy + h / 2.0,
                });
            }
        }
        _ => bail!("不支持的输出形状 {:?}", shape),
    }

    // 反 letterbox：去掉 padding 再除缩放比
    let max_x = (meta.src_w.saturating_sub(1)) as f32;
    let max_y = (meta.src_h.saturating_sub(1)) as f32;
    for d in raw.iter_mut() {
        d.x1 = ((d.x1 - meta.pad_w) / meta.ratio).clamp(0.0, max_x);
        d.y1 = ((d.y1 - meta.pad_h) / meta.ratio).clamp(0.0, max_y);
        d.x2 = ((d.x2 - meta.pad_w) / meta.ratio).clamp(0.0, max_x);
        d.y2 = ((d.y2 - meta.pad_h) / meta.ratio).clamp(0.0, max_y);
    }
    raw.retain(|d| d.x2 > d.x1 && d.y2 > d.y1);

    Ok(nms(raw, iou_threshold))
}

fn iou(a: &Detection, b: &Detection) -> f32 {
    let ix1 = a.x1.max(b.x1);
    let iy1 = a.y1.max(b.y1);
    let ix2 = a.x2.min(b.x2);
    let iy2 = a.y2.min(b.y2);
    let iw = (ix2 - ix1).max(0.0);
    let ih = (iy2 - iy1).max(0.0);
    let inter = iw * ih;
    if inter <= 0.0 {
        return 0.0;
    }
    let area_a = (a.x2 - a.x1) * (a.y2 - a.y1);
    let area_b = (b.x2 - b.x1) * (b.y2 - b.y1);
    inter / (area_a + area_b - inter)
}

/// 逐类贪心 NMS（等价于 Ultralytics 的 class-aware agnostic=False）
pub fn nms(mut dets: Vec<Detection>, iou_threshold: f32) -> Vec<Detection> {
    dets.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    let mut kept: Vec<Detection> = Vec::with_capacity(dets.len());
    for cand in dets {
        let suppressed = kept
            .iter()
            .any(|k| k.class_id == cand.class_id && iou(k, &cand) > iou_threshold);
        if !suppressed {
            kept.push(cand);
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letterbox_keeps_aspect_ratio() {
        let img = RgbImage::from_pixel(640, 480, image::Rgb([200, 100, 50]));
        let lb = letterbox(&img, 640).unwrap();
        assert_eq!(lb.data.len(), 3 * 640 * 640);
        // 640x480 -> 缩放比 1.0，上下各补 80
        assert!((lb.meta.ratio - 1.0).abs() < 1e-6);
        assert!((lb.meta.pad_h - 80.0).abs() < 1e-6);
        assert!((lb.meta.pad_w - 0.0).abs() < 1e-6);
    }

    #[test]
    fn decode_transposed_layout_restores_coords() {
        // (1, 4+nc=5, N=10)：第 0 个候选框中心 (320,320) 尺寸 100x100，类别 0 置信度 0.9
        let shape = vec![1usize, 5, 10];
        let mut flat = vec![0f32; 5 * 10];
        flat[0] = 320.0; // cx
        flat[10] = 320.0; // cy
        flat[20] = 100.0; // w
        flat[30] = 100.0; // h
        flat[40] = 0.9; // class 0 score
        let meta = LetterboxMeta {
            ratio: 1.0,
            pad_w: 0.0,
            pad_h: 0.0,
            src_w: 640,
            src_h: 640,
            input_size: 640,
        };
        let dets = decode(&flat, &shape, &meta, 1, 0.25, 0.45).unwrap();
        assert_eq!(dets.len(), 1);
        assert_eq!(dets[0].class_id, 0);
        assert!((dets[0].x1 - 270.0).abs() < 1e-3);
        assert!((dets[0].x2 - 370.0).abs() < 1e-3);
    }

    #[test]
    fn nms_suppresses_same_class_overlap() {
        let a = Detection { class_id: 1, score: 0.9, x1: 0.0, y1: 0.0, x2: 10.0, y2: 10.0 };
        let b = Detection { class_id: 1, score: 0.8, x1: 1.0, y1: 1.0, x2: 11.0, y2: 11.0 };
        let c = Detection { class_id: 2, score: 0.7, x1: 1.0, y1: 1.0, x2: 11.0, y2: 11.0 };
        let kept = nms(vec![a, b, c], 0.45);
        assert_eq!(kept.len(), 2);
    }
}
