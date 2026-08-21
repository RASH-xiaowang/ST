// ============================================================
// Windows OCR（WinRT Windows.Media.Ocr）— 扫描版 PDF / 图片文字识别
// 仅 Windows 目标启用；复用现有 windows crate，无需额外依赖。
// 流程：图片字节 → InMemoryRandomAccessStream → BitmapDecoder → SoftwareBitmap
//      → OcrEngine（优先用户语言，回退 zh-Hans）→ RecognizeAsync → 文本行
// ============================================================

use windows::core::HSTRING;
use windows::Globalization::Language;
use windows::Graphics::Imaging::BitmapDecoder;
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};

/// 对单张图片（JPEG/PNG 等 BitmapDecoder 支持的格式）执行 OCR，返回按行拼接的文本
pub fn ocr_image(image_bytes: &[u8]) -> Result<String, String> {
    if image_bytes.is_empty() {
        return Err("图片内容为空".to_string());
    }
    let stream = InMemoryRandomAccessStream::new().map_err(|e| format!("创建内存流失败: {}", e))?;
    let writer =
        DataWriter::CreateDataWriter(&stream).map_err(|e| format!("创建数据写入器失败: {}", e))?;
    writer
        .WriteBytes(image_bytes)
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
