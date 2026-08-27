// ============================================================
// Windows OCR（WinRT Windows.Media.Ocr）— 扫描版 PDF / 图片文字识别
// 仅 Windows 目标启用；复用现有 windows crate，无需额外依赖。
// 流程：图片字节 → 预压缩（超大图缩放）→ InMemoryRandomAccessStream
//      → BitmapDecoder → SoftwareBitmap → OcrEngine → RecognizeAsync → 文本行
// ============================================================

use windows::core::HSTRING;
use windows::Globalization::Language;
use windows::Graphics::Imaging::BitmapDecoder;
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};

/// OCR 前图片最大宽度（像素）。超过此宽度的图片会等比缩放，
/// 减少 OCR 处理时间 50-80%，对识别精度影响极小。
const OCR_MAX_WIDTH: u32 = 2048;

/// 对图片进行预压缩：超大图等比缩放到 OCR_MAX_WIDTH，返回 JPEG 字节。
/// 小图直接返回原数据，不做额外处理。
fn compress_for_ocr(image_bytes: &[u8]) -> Vec<u8> {
    // 尝试解码图片获取尺寸
    let Ok(img) = image::load_from_memory(image_bytes) else {
        // 解码失败（格式不支持等），返回原数据，让 OCR 引擎自己处理
        return image_bytes.to_vec();
    };
    let w = img.width();
    let h = img.height();

    // 小图不需要压缩
    if w <= OCR_MAX_WIDTH {
        return image_bytes.to_vec();
    }

    // 等比缩放
    let new_h = (h as f64 * OCR_MAX_WIDTH as f64 / w as f64) as u32;
    let resized = img.resize_exact(OCR_MAX_WIDTH, new_h, image::imageops::FilterType::Triangle);

    // 编码为 JPEG（比 PNG 小 5-10x，OCR 不需要无损质量）
    let mut buf = std::io::Cursor::new(Vec::new());
    if resized.write_to(&mut buf, image::ImageFormat::Jpeg).is_ok() {
        log::info!(
            "图片预压缩: {}x{} → {}x{} ({}KB → {}KB)",
            w,
            h,
            OCR_MAX_WIDTH,
            new_h,
            image_bytes.len() / 1024,
            buf.get_ref().len() / 1024
        );
        buf.into_inner()
    } else {
        // 编码失败，返回原数据
        image_bytes.to_vec()
    }
}

/// 对单张图片（JPEG/PNG 等 BitmapDecoder 支持的格式）执行 OCR，返回按行拼接的文本
/// 自动对超大图片进行预压缩，减少 OCR 处理时间。
pub fn ocr_image(image_bytes: &[u8]) -> Result<String, String> {
    if image_bytes.is_empty() {
        return Err("图片内容为空".to_string());
    }

    // 预压缩：超大图缩放到 2048px 宽度
    let compressed = compress_for_ocr(image_bytes);

    let stream = InMemoryRandomAccessStream::new().map_err(|e| format!("创建内存流失败: {}", e))?;
    let writer =
        DataWriter::CreateDataWriter(&stream).map_err(|e| format!("创建数据写入器失败: {}", e))?;
    writer
        .WriteBytes(&compressed)
        .map_err(|e| format!("写入图片字节失败: {}", e))?;
    let _ = writer
        .StoreAsync()
        .map_err(|e| e.to_string())?
        .join()
        .map_err(|e| format!("刷新流失败: {}", e))?;
    let _ = writer.DetachStream();
    stream
        .Seek(0)
        .map_err(|e| format!("重置流位置失败: {}", e))?;

    let decoder = BitmapDecoder::CreateAsync(&stream)
        .map_err(|e| e.to_string())?
        .join()
        .map_err(|e| format!("创建位图解码器失败: {}", e))?;
    let bitmap = decoder
        .GetSoftwareBitmapAsync()
        .map_err(|e| e.to_string())?
        .join()
        .map_err(|e| format!("解码位图失败: {}", e))?;

    // 优先使用用户配置的 OCR 语言（中文系统通常已含简体中文），失败再显式指定 zh-Hans
    let engine = OcrEngine::TryCreateFromUserProfileLanguages()
        .or_else(|_| {
            OcrEngine::TryCreateFromLanguage(&Language::CreateLanguage(&HSTRING::from("zh-Hans"))?)
        })
        .map_err(|e| format!("OCR 引擎初始化失败（系统未安装 OCR 语言包）: {}", e))?;

    let result = engine
        .RecognizeAsync(&bitmap)
        .map_err(|e| e.to_string())?
        .join()
        .map_err(|e| format!("文字识别失败: {}", e))?;

    let mut out = String::new();
    let lines = result
        .Lines()
        .map_err(|e| format!("读取识别结果失败: {}", e))?;
    for line in lines {
        if let Ok(text) = line.Text() {
            out.push_str(&text.to_string());
            out.push('\n');
        }
    }
    Ok(out)
}
