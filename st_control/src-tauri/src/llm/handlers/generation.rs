// ============================================================
// 大模型管理 — IPC 命令：图像 / 视频生成
// 自 handlers.rs 拆分：提供方/模型解析 + 参数规整 + 生成调用。
// ============================================================

use crate::llm::client;
use crate::llm::config;
use crate::llm::types::{ImageGenRequest, ImageGenResult, VideoGenRequest, VideoGenResult};

/// 图像生成：选择提供方与模型，调用 OpenAI 兼容的 /images/generations 接口。
/// 仅当模型被标注为「生图」类型时使用，但此处不强校验，交由前端按类型路由。
#[tauri::command]
pub async fn generate_image(request: ImageGenRequest) -> Result<ImageGenResult, String> {
    let cfg = config::load_config();

    // 1. 选定提供方
    let provider_id = match &request.provider_id {
        Some(id) => id.clone(),
        None => cfg
            .default_provider_id
            .clone()
            .ok_or_else(|| "未配置默认提供方，请在「接入配置」中设置".to_string())?,
    };
    let provider = config::find_provider(&cfg, &provider_id)
        .ok_or_else(|| "指定的提供方不存在".to_string())?
        .clone();

    if !provider.enabled {
        return Err("该提供方已被禁用".to_string());
    }

    // 2. 选定模型
    let model = request
        .model
        .clone()
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| provider.default_model.clone());
    if model.is_empty() {
        return Err("未指定模型，且提供方未配置默认模型".to_string());
    }

    // 3. 参数规整
    let n = request.n.unwrap_or(1).clamp(1, 4);
    let size = request.size.as_deref();

    // 4. 执行生成
    let urls = client::generate_image(&provider, &model, &request.prompt, n, size).await?;

    Ok(ImageGenResult {
        provider_id,
        provider_name: provider.name,
        model,
        urls,
    })
}

/// 视频生成：调用 OpenAI 兼容的 /videos/generations 接口。
/// 仅当模型被标注为「视频」类型时使用，但此处不强校验，交由前端按类型路由。
#[tauri::command]
pub async fn generate_video(request: VideoGenRequest) -> Result<VideoGenResult, String> {
    let cfg = config::load_config();

    let provider_id = match &request.provider_id {
        Some(id) => id.clone(),
        None => cfg
            .default_provider_id
            .clone()
            .ok_or_else(|| "未配置默认提供方，请在「接入配置」中设置".to_string())?,
    };
    let provider = config::find_provider(&cfg, &provider_id)
        .ok_or_else(|| "指定的提供方不存在".to_string())?
        .clone();

    if !provider.enabled {
        return Err("该提供方已被禁用".to_string());
    }

    let model = request
        .model
        .clone()
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| provider.default_model.clone());
    if model.is_empty() {
        return Err("未指定模型，且提供方未配置默认模型".to_string());
    }

    let n = request.n.unwrap_or(1).clamp(1, 4);
    let urls = client::generate_video(&provider, &model, &request.prompt, n).await?;

    Ok(VideoGenResult {
        provider_id,
        provider_name: provider.name,
        model,
        urls,
    })
}
