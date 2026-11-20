// 12 类病虫害（与 backend/models/dataset/pest_classes.json 一一对应）
// 后端类别文件只给了 zh_name / en_name / type，处置建议按类型统一兜底，
// 精细文案后续可在后台维护，这里只保证离线模式下不缺字段。

export type PestType = 'insect' | 'disease' | 'virus' | 'physiological' | 'unknown';
export type PestSeverity = 'low' | 'medium' | 'high';

export interface PestClassInfo {
  name: string;
  enName: string;
  type: PestType;
}

export const PEST_CLASSES: Record<number, PestClassInfo> = {
  0: { name: '马铃薯早疫病', enName: 'Potato___Early_blight', type: 'disease' },
  1: { name: '黄化曲叶病毒', enName: 'Tomato__Tomato_YellowLeaf__Curl_Virus', type: 'virus' },
  2: { name: '番茄早疫病', enName: 'Tomato_Early_blight', type: 'disease' },
  3: { name: '番茄靶斑病', enName: 'Tomato__Target_Spot', type: 'disease' },
  4: { name: '马铃薯晚疫病', enName: 'Potato___Late_blight', type: 'disease' },
  5: { name: '番茄叶霉病', enName: 'Tomato_Leaf_Mold', type: 'disease' },
  6: { name: '红蜘蛛', enName: 'Tomato_Spider_mites_Two_spotted_spider_mite', type: 'insect' },
  7: { name: '番茄斑枯病', enName: 'Tomato_Septoria_leaf_spot', type: 'disease' },
  8: { name: '花叶病毒', enName: 'Tomato__Tomato_mosaic_virus', type: 'virus' },
  9: { name: '辣椒细菌性斑点病', enName: 'Pepper__bell___Bacterial_spot', type: 'disease' },
  10: { name: '番茄细菌性斑点病', enName: 'Tomato_Bacterial_spot', type: 'disease' },
  11: { name: '番茄晚疫病', enName: 'Tomato_Late_blight', type: 'disease' },
};

export interface PestCareAdvice {
  treatment: string;
  prevention: string;
  severity: PestSeverity;
}

const ADVICE_BY_TYPE: Record<PestType, PestCareAdvice> = {
  insect: {
    treatment: '隔离病株，用湿布或清水冲洗虫体，必要时喷施吡虫啉/阿维菌素，7 天后复喷一次',
    prevention: '保持通风，定期检查叶背，避免环境过于干燥闷热',
    severity: 'medium',
  },
  disease: {
    treatment: '剪除并销毁病叶，喷施多菌灵或百菌清 500 倍液，每 7 天一次，连续 3 次',
    prevention: '加强通风透光，避免叶面长时间积水，合理施肥增强抗性',
    severity: 'medium',
  },
  virus: {
    treatment: '病毒病无特效药，立即隔离或拔除病株，并防治蚜虫、白粉虱等传播媒介',
    prevention: '使用无毒种苗，工具消毒，及时清除周边杂草宿主',
    severity: 'high',
  },
  physiological: {
    treatment: '排查光照、水分、温度与肥害，调整养护条件后观察新叶恢复情况',
    prevention: '稳定环境参数，避免忽干忽湿与强光暴晒',
    severity: 'low',
  },
  unknown: {
    treatment: '建议拍摄更清晰的病部特写后重新识别，或咨询社区专家',
    prevention: '保持良好养护习惯，定期观察植株状态',
    severity: 'low',
  },
};

export function getPestInfo(classId: number): PestClassInfo {
  return PEST_CLASSES[classId] ?? { name: '未知病虫害', enName: '', type: 'unknown' };
}

export function getPestAdvice(type: PestType): PestCareAdvice {
  return ADVICE_BY_TYPE[type] ?? ADVICE_BY_TYPE.unknown;
}
