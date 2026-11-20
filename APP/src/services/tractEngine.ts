// tract 端侧推理引擎的 JS 桥接层
// 原生实现：APP/native/tract-engine（Rust）+ Android TractEngineModule（Kotlin/JNI）
import { NativeModules, Platform } from 'react-native';

export interface TractDetection {
  classId: number;
  score: number;
  /** 原图像素坐标 */
  x1: number;
  y1: number;
  x2: number;
  y2: number;
}

export interface TractResult {
  detections: TractDetection[];
  width: number;
  height: number;
  elapsedMs: number;
}

type TractNativeModule = {
  isAvailable(): Promise<boolean>;
  version(): Promise<string | null>;
  loadFromAsset(assetName: string, inputSize: number, numClasses: number): Promise<boolean>;
  load(modelPath: string, inputSize: number, numClasses: number): Promise<boolean>;
  detect(
    modelKey: string,
    imagePath: string,
    confThreshold: number,
    iouThreshold: number
  ): Promise<string>;
  dispose(): Promise<boolean>;
};

const nativeModule: TractNativeModule | null =
  Platform.OS === 'android' ? (NativeModules as Record<string, TractNativeModule>).TractEngine ?? null : null;

/** 原生引擎是否可用（iOS / Web / 未编译 .so 时为 false） */
export const isTractAvailable = nativeModule != null;

/** 把 ImagePicker 返回的 `file://` URI 转成原生可读的绝对路径 */
export function toFilePath(uri: string): string {
  let path = uri;
  if (path.startsWith('file://')) {
    path = path.slice('file://'.length);
  }
  try {
    return decodeURIComponent(path);
  } catch {
    return path;
  }
}

export async function getTractVersion(): Promise<string | null> {
  if (!nativeModule) return null;
  try {
    return await nativeModule.version();
  } catch {
    return null;
  }
}

/** 从 apk assets/models/ 加载模型（首次会复制到私有目录） */
export async function loadModelFromAsset(
  assetName: string,
  inputSize: number,
  numClasses: number
): Promise<boolean> {
  if (!nativeModule) return false;
  try {
    return await nativeModule.loadFromAsset(assetName, inputSize, numClasses);
  } catch (error) {
    console.warn('[TractEngine] 模型加载失败:', assetName, error);
    return false;
  }
}

/** 从沙盒绝对路径加载模型（模型热更新场景） */
export async function loadModelFromPath(
  modelPath: string,
  inputSize: number,
  numClasses: number
): Promise<boolean> {
  if (!nativeModule) return false;
  try {
    return await nativeModule.load(modelPath, inputSize, numClasses);
  } catch (error) {
    console.warn('[TractEngine] 模型加载失败:', modelPath, error);
    return false;
  }
}

/** 推理单张图片，失败返回 null（调用方负责降级） */
export async function detectFile(
  modelKey: string,
  imagePath: string,
  confThreshold = 0.35,
  iouThreshold = 0.45
): Promise<TractResult | null> {
  if (!nativeModule) return null;
  try {
    const json = await nativeModule.detect(modelKey, imagePath, confThreshold, iouThreshold);
    if (!json) return null;
    return JSON.parse(json) as TractResult;
  } catch (error) {
    console.warn('[TractEngine] 推理失败:', error);
    return null;
  }
}

export async function disposeEngine(): Promise<void> {
  if (!nativeModule) return;
  try {
    await nativeModule.dispose();
  } catch {
    // 忽略：进程退出时释放失败不影响业务
  }
}
