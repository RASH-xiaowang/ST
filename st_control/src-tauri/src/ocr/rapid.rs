// ============================================================
// 开源 OCR 预检引擎（RapidOCR PP-OCRv6，Apache-2.0）
// 本地 ONNX 推理：检测 + 识别中文/英文文本行。
// 模型不随程序打包：首次使用通过 ModelCache 从 ModelScope 自动下载
// 到应用数据目录（%APPDATA%/st-control/rapidocr-models），之后离线可用。
// 用途：在调用 TextIn 证件分类前预检图片是否含文字，过滤无意义图片。
// ============================================================

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rapidocr_core::config::PipelineConfig;
use rapidocr_core::model::{
    model_set_by_name, ModelCache, ModelDownloadMode, DEFAULT_MODEL_SET_NAME,
};
use rapidocr_core::RapidOcr;

/// 缓存中的一个引擎实例（模型目录变化时重建）
#[derive(Clone)]
pub struct PrecheckSlot {
    pub model_dir: PathBuf,
    pub ocr: Arc<Mutex<RapidOcr>>,
}

/// 默认模型缓存目录：%APPDATA%/st-control/rapidocr-models
pub fn default_model_dir() -> PathBuf {
    crate::common::st_data_dir().join("rapidocr-models")
}

/// 解析配置中的模型目录（空 = 默认）
pub fn resolve_model_dir(cfg_dir: &str) -> PathBuf {
    let t = cfg_dir.trim();
    if t.is_empty() {
        default_model_dir()
    } else {
        PathBuf::from(t)
    }
}

/// 初始化引擎：校验/下载模型 → 加载 ONNX 会话。
/// 阻塞操作（含可能的模型下载），务必在 spawn_blocking 中调用。
pub fn init_engine(model_dir: &Path) -> Result<PrecheckSlot, String> {
    let model_set = model_set_by_name(DEFAULT_MODEL_SET_NAME)
        .ok_or_else(|| format!("未知模型集: {DEFAULT_MODEL_SET_NAME}"))?;
    // 预检只需 检测 + 识别，不需要方向分类模型（减少下载体积）
    let pipeline = PipelineConfig::without_cls();
    let cache = ModelCache::new(model_dir.to_path_buf());
    cache
        .ensure_model_set_for_pipeline(model_set, pipeline, ModelDownloadMode::Missing)
        .map_err(|e| {
            format!(
                "RapidOCR 模型初始化失败（首次使用需联网下载到 {}）: {e}",
                model_dir.display()
            )
        })?;
    let cfg = cache.config_for(model_set).with_pipeline(pipeline);
    let ocr = RapidOcr::from_config(cfg).map_err(|e| {
        format!("加载 RapidOCR 引擎失败（请确认 onnxruntime.dll 已随程序部署）: {e}")
    })?;
    Ok(PrecheckSlot {
        model_dir: model_dir.to_path_buf(),
        ocr: Arc::new(Mutex::new(ocr)),
    })
}

/// 对图片文件执行 OCR，返回按行拼接的文本（空格行过滤）
pub fn ocr_file(path: &Path, ocr: &Arc<Mutex<RapidOcr>>) -> Result<String, String> {
    let mut guard = ocr.lock().map_err(|_| "OCR 引擎锁获取失败".to_string())?;
    let output = guard
        .run_path(path)
        .map_err(|e| format!("OCR 识别失败: {e}"))?;
    let mut text = String::new();
    for line in output.lines {
        let t = line.text.trim();
        if !t.is_empty() {
            text.push_str(t);
            text.push('\n');
        }
    }
    Ok(text)
}
