/* ============================================================
 * OCR — 展示纯函数与常量
 * 自 OcrPanel.svelte 下沉：状态元数据、分类标签、JSON 美化。
 * 不依赖组件状态，可独立单测。
 * ============================================================ */

/** OCR 资源状态元信息（label + 徽章类） */
export const STATUS_META: Record<string, { label: string; cls: string }> = {
  pending: { label: '待处理', cls: 'outline' },
  processing: { label: '处理中', cls: 'secondary' },
  filtered: { label: '已过滤（无文本）', cls: 'secondary' },
  saved: { label: '已分类', cls: 'secondary' },
  success: { label: '识别成功', cls: 'default' },
  ocr_failed: { label: 'OCR 失败', cls: 'secondary' },
  failed: { label: '失败', cls: 'destructive' },
};

/** 分类展示顺序 */
export const CATEGORY_ORDER = [
  'id_card', 'id_card_front', 'id_card_back', 'id_card_front_and_back',
  'drive_license', 'vehicle_license', 'bank_card', 'business_card',
  'business_license', 'passport', 'hongkong_idcard', 'macau_id_card',
  'social_security_cards', 'family_register', 'marriage_certificate',
  'divorce_certificate', 'house_property_owner_ship', 'real_estate',
  'opening_license', 'organization_certificate', 'vehicle_certificate',
  'vehicle_registration', 'tax_certificate', 'other',
];

/** 常用测试端点 */
export const COMMON_ENDPOINTS = [
  'id_card', 'driver_license', 'vehicle_license', 'bank_card', 'business_card',
  'business_license', 'passport', 'hk_id_card', 'mac_id_card',
  'social_security_card', 'organization_code_certificate', 'account_opening_permit',
];

/** JSON 美化（空/非法原样或占位） */
export function prettyJson(s: string): string {
  if (!s || s === '{}') return '（空）';
  try {
    return JSON.stringify(JSON.parse(s), null, 2);
  } catch {
    return s;
  }
}

/** 状态 → 中文标签（未知原样） */
export function statusLabel(st: string): string {
  return STATUS_META[st]?.label ?? st;
}

/** 状态 → 徽章类（未知 outline） */
export function statusCls(st: string): 'default' | 'secondary' | 'destructive' | 'outline' {
  return (STATUS_META[st]?.cls ?? 'outline') as 'default' | 'secondary' | 'destructive' | 'outline';
}

/** 分类 → 中文标签（空 → 未分类，未知原样） */
export function catLabel(cat: string): string {
  if (!cat) return '未分类';
  const map: Record<string, string> = {
    id_card: '身份证', id_card_front: '身份证(人像面)', id_card_back: '身份证(国徽面)',
    id_card_front_and_back: '身份证(正反面)', drive_license: '驾驶证', vehicle_license: '行驶证',
    bank_card: '银行卡', business_card: '名片', business_license: '营业执照', passport: '护照',
    hongkong_idcard: '香港身份证', macau_id_card: '澳门身份证', social_security_cards: '社保卡',
    family_register: '户口本', marriage_certificate: '结婚证', divorce_certificate: '离婚证',
    house_property_owner_ship: '房产证', real_estate: '不动产证', opening_license: '开户许可证',
    organization_certificate: '组织机构代码证', vehicle_certificate: '车辆合格证',
    vehicle_registration: '车辆登记证', tax_certificate: '税务登记证', other: '其他',
  };
  return map[cat] ?? cat;
}
