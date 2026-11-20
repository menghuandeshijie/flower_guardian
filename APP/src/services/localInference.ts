// 本地推理编排层：模型懒加载 + 多次调用复用 + 结果映射为业务结构
// 所有函数在「引擎不可用 / 模型缺失 / 推理失败」时返回 null，由调用方走在线或降级逻辑。
import { PLANT_CLASSES, getPlantCareTips, getPlantName } from '../data/plantClasses';
import { PEST_CLASSES, getPestAdvice, getPestInfo, PestSeverity, PestType } from '../data/pestClasses';
import * as tract from './tractEngine';

/** 与导出脚本 export_onnx.py 约定的文件名（放在 android/app/src/main/assets/models/） */
export const PLANT_MODEL_ASSET = 'plant_yolo11n.onnx';
export const PEST_MODEL_ASSET = 'pest_yolo11n.onnx';

const INPUT_SIZE = 640;
const IOU_THRESHOLD = 0.45;
/** 植物识别放宽阈值（植物主体通常占画面比例大，误检代价低） */
const PLANT_CONF_THRESHOLD = 0.35;
/** 病虫害沿用后端 PestRecognitionService.CONFIDENCE_THRESHOLD = 0.6 */
const PEST_CONF_THRESHOLD = 0.6;

const PLANT_CLASS_COUNT = Object.keys(PLANT_CLASSES).length;
const PEST_CLASS_COUNT = Object.keys(PEST_CLASSES).length;

export interface LocalPlantResult {
  id: string;
  name: string;
  confidence: number;
  care_tips: string;
}

export interface LocalPestResult {
  id: string;
  name: string;
  confidence: number;
  type: PestType;
  treatment: string;
  prevention: string;
  severity: PestSeverity;
}

type ModelKind = 'plant' | 'pest';
type ModelState = 'unknown' | 'ready' | 'unavailable';

const state: Record<ModelKind, ModelState> = { plant: 'unknown', pest: 'unknown' };
const pending: Record<ModelKind, Promise<boolean> | null> = { plant: null, pest: null };

function modelAsset(kind: ModelKind): string {
  return kind === 'plant' ? PLANT_MODEL_ASSET : PEST_MODEL_ASSET;
}

async function load(kind: ModelKind): Promise<boolean> {
  const ok = await tract.loadModelFromAsset(
    modelAsset(kind),
    INPUT_SIZE,
    kind === 'plant' ? PLANT_CLASS_COUNT : PEST_CLASS_COUNT
  );
  state[kind] = ok ? 'ready' : 'unavailable';
  if (!ok) {
    console.warn(`[LocalInference] 模型不可用: ${modelAsset(kind)}（请先执行 export_onnx.py）`);
  }
  return ok;
}

/** 懒加载并复用已加载的模型；并发调用会共享同一个 Promise */
export async function ensureModel(kind: ModelKind): Promise<boolean> {
  if (!tract.isTractAvailable) return false;
  if (state[kind] === 'ready') return true;
  if (state[kind] === 'unavailable') return false;
  if (!pending[kind]) {
    pending[kind] = load(kind).finally(() => {
      pending[kind] = null;
    });
  }
  return pending[kind] as Promise<boolean>;
}

/** 取分数最高的检测框（与后端 argmax(conf) 行为一致） */
function topDetection(result: tract.TractResult | null): tract.TractDetection | null {
  if (!result || result.detections.length === 0) return null;
  return result.detections.reduce((best, cur) => (cur.score > best.score ? cur : best));
}

export async function detectPlantLocal(imageUri: string): Promise<LocalPlantResult | null> {
  if (!(await ensureModel('plant'))) return null;

  const result = await tract.detectFile(
    PLANT_MODEL_ASSET,
    tract.toFilePath(imageUri),
    PLANT_CONF_THRESHOLD,
    IOU_THRESHOLD
  );
  const best = topDetection(result);
  if (!best) return null;

  return {
    id: String(best.classId),
    name: getPlantName(best.classId),
    confidence: best.score,
    care_tips: getPlantCareTips(best.classId),
  };
}

export async function detectPestLocal(imageUri: string): Promise<LocalPestResult | null> {
  if (!(await ensureModel('pest'))) return null;

  const result = await tract.detectFile(
    PEST_MODEL_ASSET,
    tract.toFilePath(imageUri),
    PEST_CONF_THRESHOLD,
    IOU_THRESHOLD
  );
  const best = topDetection(result);
  // 推理过程失败（引擎异常/文件不可读）时返回 null，交给上层降级；
  // 推理成功但无框高于阈值时返回「未识别」，与后端 id=-1 语义一致。
  if (!result) return null;
  if (!best) {
    return {
      id: '-1',
      name: '未识别',
      confidence: 0,
      type: 'unknown',
      treatment: '置信度较低，请拍摄更清晰的照片',
      prevention: getPestAdvice('unknown').prevention,
      severity: 'low',
    };
  }

  const info = getPestInfo(best.classId);
  const advice = getPestAdvice(info.type);
  return {
    id: String(best.classId),
    name: info.name,
    confidence: best.score,
    type: info.type,
    treatment: advice.treatment,
    prevention: advice.prevention,
    severity: advice.severity,
  };
}

/** 引擎自检信息，供「设置/关于」页面展示 */
export async function getLocalInferenceStatus(): Promise<{
  available: boolean;
  engineVersion: string | null;
  plantModel: ModelState;
  pestModel: ModelState;
}> {
  return {
    available: tract.isTractAvailable,
    engineVersion: await tract.getTractVersion(),
    plantModel: state.plant,
    pestModel: state.pest,
  };
}
