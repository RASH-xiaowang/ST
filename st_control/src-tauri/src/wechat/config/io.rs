// ============================================================
// 微信配置 — 加载 / 保存 / 补丁
// 自 config.rs 拆分：缓存加载、配置持久化与密钥补丁。
// ============================================================

use std::path::{Path, PathBuf};

use super::{
    app_base_dir, auto_detect_db_dir, default_decoded_image_dir, default_decrypted_dir,
    default_st_result_dir, normalize_wxid_dir, scan_accounts, KeyConfigPatch, RawConfig,
    WeChatConfig, DEFAULT_IMAGE_XOR_KEY, DEFAULT_MONITOR_CACHE, DEFAULT_PROCESS,
};

/// 全局配置缓存：避免每次 IPC 调用都重复读取文件和扫描目录。
/// 用 Mutex<Option<..>> 以便保存后调用 refresh_cache() 使新值立即可见
/// （OnceLock 无法重置，曾导致保存配置后 apply_api_settings 读到旧值）。
static CONFIG_CACHE: std::sync::Mutex<Option<WeChatConfig>> = std::sync::Mutex::new(None);

impl WeChatConfig {
    /// 加载微信配置（带缓存，避免重复 I/O 和目录扫描）
    ///
    /// 解析优先级:
    /// 1. 环境变量 `ST_WECHAT_DB_DIR`
    /// 2. `<应用基目录>/config.json`（唯一配置文件；路径字段可为
    ///    绝对路径、相对路径（相对应用基目录）或留空=自动检测/默认值）
    /// 3. 自动检测（微信根目录 ini → 最活跃账号 → 系统探测）
    pub fn load() -> std::io::Result<Self> {
        // 缓存命中直接返回克隆，避免重复文件读取和目录扫描
        {
            let guard = CONFIG_CACHE.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(cached) = guard.as_ref() {
                return Ok(cached.clone());
            }
        }

        let result = Self::load_uncached()?;
        *CONFIG_CACHE.lock().unwrap_or_else(|e| e.into_inner()) = Some(result.clone());
        Ok(result)
    }

    /// 保存配置后刷新缓存，使后续 load() 返回最新值
    pub fn refresh_cache() {
        let fresh = Self::load_uncached().ok();
        *CONFIG_CACHE.lock().unwrap_or_else(|e| e.into_inner()) = fresh;
    }

    /// 实际加载逻辑（无缓存，供内部调用）
    fn load_uncached() -> std::io::Result<Self> {
        let app_dir = app_base_dir();

        // 尝试从 config.json 加载（唯一位置：应用基目录下）
        let raw = load_raw_config(&app_dir);
        let mut config = raw.unwrap_or_default();

        // 环境变量覆盖
        if let Ok(env_db) = std::env::var("ST_WECHAT_DB_DIR") {
            if !env_db.is_empty() {
                config.db_dir = Some(env_db);
            }
        }

        // 解析 db_dir（优先级：config.db_dir(绝对/相对) > wechat_root 扫描 > 最活跃账号 > 系统自动检测 > 报错）
        let db_dir = match config.db_dir {
            Some(ref d) if !d.contains("your_wxid") && resolve_maybe_rel(&app_dir, d).is_dir() => {
                let p = resolve_maybe_rel(&app_dir, d);
                log::info!("使用配置中的 db_dir: {}", p.display());
                p
            }
            _ => {
                // 尝试从保存的 wechat_root 扫描（相对路径相对应用基目录）
                let from_root = config.wechat_root.as_ref().and_then(|root| {
                    let p = resolve_maybe_rel(&app_dir, root);
                    if p.is_dir() {
                        let accounts = scan_accounts(&p);
                        accounts
                            .into_iter()
                            .max_by_key(|a| a.last_active)
                            .map(|a| PathBuf::from(a.db_dir))
                    } else {
                        None
                    }
                });
                match from_root {
                    Some(detected) => {
                        log::info!("从配置的 wechat_root 扫描到: {}", detected.display());
                        detected
                    }
                    None => {
                        // 本机全部账号中选最活跃（≈当前登录账号），跨机器可移植
                        let from_accounts = super::detect_accounts()
                            .into_iter()
                            .next()
                            .map(|a| PathBuf::from(a.db_dir));
                        match from_accounts {
                            Some(detected) => {
                                log::info!(
                                    "自动检测到最活跃微信账号数据目录: {}",
                                    detected.display()
                                );
                                detected
                            }
                            None => {
                                // 最后尝试系统级自动检测
                                match auto_detect_db_dir() {
                                    Some(detected) => {
                                        log::info!(
                                            "自动检测到微信数据目录: {}",
                                            detected.display()
                                        );
                                        detected
                                    }
                                    None => {
                                        return Err(std::io::Error::new(
                                            std::io::ErrorKind::NotFound,
                                            "未找到微信数据目录。请在设置中指定微信根目录，或填写正确的 db_dir",
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        };

        // 推导 wechat_base_dir (db_dir 的父目录)
        let wechat_base_dir = if db_dir.file_name().and_then(|n| n.to_str()) == Some("db_storage") {
            db_dir.parent().unwrap_or(&db_dir).to_path_buf()
        } else {
            db_dir.clone()
        };

        // 解析各路径：空/旧默认值 → 统一默认目录；自定义支持绝对路径或相对应用基目录
        let keys_file = match config.keys_file.as_deref() {
            None | Some("all_keys.json") | Some("") => {
                default_st_result_dir().join("all_keys.json")
            }
            Some(custom) => resolve_maybe_rel(&app_dir, custom),
        };
        let decrypted_dir = match config.decrypted_dir.as_deref() {
            // 旧默认值 / 空值 → 使用 <base>/data/wechat/decrypted
            None | Some("decrypted") | Some("st_decrypted") | Some("") => default_decrypted_dir(),
            Some(custom) => resolve_maybe_rel(&app_dir, custom),
        };
        let decoded_image_dir = match config.decoded_image_dir.as_deref() {
            // 旧默认值 / 空值 → 使用 <base>/data/wechat/decoded_images
            None | Some("decoded_images") | Some("st_decoded_images") | Some("") => {
                default_decoded_image_dir()
            }
            Some(custom) => resolve_maybe_rel(&app_dir, custom),
        };
        let wechat_process = config
            .wechat_process
            .unwrap_or_else(|| DEFAULT_PROCESS.to_string());

        // 旧目录兼容回退：旧 %APPDATA%/st_result 仍存在（迁移未运行/未完成）时，
        // 统一默认目录的这三项沿用旧位置，保证功能与数据连续；迁移完成后
        // 旧目录被改名，自动切换到统一目录。仅对默认路径生效，自定义路径不受影响。
        let default_root = default_st_result_dir();
        let keys_file = if keys_file.starts_with(&default_root) {
            legacy_default_if_present("all_keys.json").unwrap_or(keys_file)
        } else {
            keys_file
        };
        let decrypted_dir = if decrypted_dir.starts_with(&default_root) {
            legacy_default_if_present("decrypted").unwrap_or(decrypted_dir)
        } else {
            decrypted_dir
        };
        let decoded_image_dir = if decoded_image_dir.starts_with(&default_root) {
            legacy_default_if_present("decoded_images").unwrap_or(decoded_image_dir)
        } else {
            decoded_image_dir
        };
        let monitor_cache_dir = decrypted_dir.join(DEFAULT_MONITOR_CACHE);

        // 确保目录存在
        std::fs::create_dir_all(&decrypted_dir).ok();
        std::fs::create_dir_all(&decoded_image_dir).ok();
        std::fs::create_dir_all(&monitor_cache_dir).ok();

        Ok(Self {
            db_dir,
            wechat_base_dir,
            decrypted_dir,
            decoded_image_dir,
            monitor_cache_dir,
            keys_file,
            image_aes_key: config.image_aes_key,
            image_xor_key: config.image_xor_key.unwrap_or(DEFAULT_IMAGE_XOR_KEY),
            wechat_process,
            key_format: config.key_format,
            api_enabled: config.api_enabled.unwrap_or(true),
            api_port: config.api_port.unwrap_or(5032),
            api_token: config.api_token.filter(|t| !t.is_empty()),
        })
    }

    /// 获取 wxid（从 wechat_base_dir 目录名，自动剥离实例后缀）
    ///
    /// 微信 4.0 目录名格式：`wxid_xxxxxx[_<实例后缀>]`，其中 `wxid_xxxxxx` 才是
    /// 真实 wxid（不含下划线），实例后缀有 `_f312` / `_9bcd` / `_a8ef` 等格式。
    /// 本函数从第二个下划线起整体剥离，以匹配数据库中的实际 wxid。
    pub fn wxid(&self) -> Option<String> {
        self.wechat_base_dir
            .file_name()
            .and_then(|n| n.to_str())
            .map(normalize_wxid_dir)
    }

    /// 检查 all_keys.json 是否存在
    pub fn has_keys(&self) -> bool {
        self.keys_file.exists()
    }
}

/// 路径解析：绝对路径原样返回；相对路径相对应用基目录拼接。
fn resolve_maybe_rel(app_dir: &Path, s: &str) -> PathBuf {
    let p = Path::new(s.trim());
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        app_dir.join(p)
    }
}

/// 旧目录兼容回退：%APPDATA%/st_result 下旧位置仍存在时返回该路径。
/// （启动迁移完成后旧目录被改名，本函数自然返回 None。）
fn legacy_default_if_present(legacy_rel: &str) -> Option<PathBuf> {
    let legacy = dirs::data_dir()?.join("st_result").join(legacy_rel);
    legacy.exists().then_some(legacy)
}

/// 加载 config.json 原始配置（唯一位置：应用基目录下）
fn load_raw_config(app_dir: &Path) -> Option<RawConfig> {
    let path = app_dir.join("config.json");
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(cfg) = serde_json::from_str(&content) {
                return Some(cfg);
            }
            log::warn!("config.json 解析失败，使用默认配置: {}", path.display());
        }
    }

    // 尝试环境变量
    if let Ok(json) = std::env::var("ST_WECHAT_CONFIG_JSON") {
        if let Ok(cfg) = serde_json::from_str::<RawConfig>(&json) {
            return Some(cfg);
        }
    }

    None
}

/// 获取 config.json 文件路径 (应用基目录下)
pub fn get_config_path() -> PathBuf {
    app_base_dir().join("config.json")
}

/// 从文件加载原始配置 (供 IPC 读取)
pub fn load_raw_config_public() -> Option<RawConfig> {
    load_raw_config(&app_base_dir())
}

/// 保存配置到 config.json
pub fn save_config(raw: &RawConfig) -> std::io::Result<()> {
    let path = get_config_path();
    let json = serde_json::to_string_pretty(raw)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&path, json)?;
    log::info!("微信配置已保存到 {:?}", path);
    Ok(())
}

/// 将自动获取的密钥写入 config.json（保留其余字段）
pub fn patch_config(patch: KeyConfigPatch<'_>) -> std::io::Result<()> {
    let mut raw = load_raw_config(&app_base_dir()).unwrap_or_default();
    if let Some(v) = patch.db_dir {
        raw.db_dir = Some(v.to_string());
    }
    if let Some(v) = patch.db_enc_key {
        raw.db_enc_key = Some(v.to_string());
    }
    if let Some(v) = patch.image_aes_key {
        raw.image_aes_key = Some(v.to_string());
    }
    if let Some(v) = patch.image_xor_key {
        raw.image_xor_key = Some(v);
    }
    save_config(&raw)
}
