<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import WechatHoverButton from './WechatHoverButton.svelte';
  import { safeParseInt } from '../utils';
  import { errText } from '../utils/format';
  import { copyText } from '../../clipboard';
  import type { AutoDbKeyResult, AutoImgKeyResult, DetectedAccount, SttDownloadProgress, SttStatus, WechatAccountStatus, WechatKeysInfo, WeChatOpProgress } from '../types';
  import {
    applyApiSettings,
    autoGetDbKey,
    autoGetImageKey,
    autoGetWechatKeys,
    decodeAllImages as callDecodeAllImages,
    decryptAllDatabases,
    detectWechatAccounts,
    downloadLocalSttModel,
    generateKeysFile,
    getCdnImageStatus,
    getLocalSttStatus,
    getWechatAccountStatus,
    getWechatConfig,
    getWechatKeysInfo,
    importWechatBackup,
    saveWechatConfig,
    setCdnImageEnabled,
    setCdnImageLocalDecrypt,
    setLocalSttConfig,
    switchWechatAccountToLive,
    verifyDatabaseKey,
    verifyImageKey as callVerifyImageKey,
  } from '../services/ipc';
  import { listen } from '@tauri-apps/api/event';
  import { open } from '@tauri-apps/plugin-dialog';
  import { autoGetDbKeyV2 } from '../services/ipc';
  import { fmtLastActive, generateApiToken as newApiToken } from '../utils/security';

  // 通用工具
  function logError(context: string, err: unknown) {
    console.error(`[WeChatConfig] ${context}:`, err);
  }
  // ─── 后端解析出的固定输出路径（统一目录方案：<应用目录>/data/wechat 下，
  //      由 get_wechat_config 返回 resolved 字段，杜绝前端硬编码绝对路径） ───
  let fixedDecryptedDir = $state('');
  let fixedDecodedImageDir = $state('');
  let fixedKeysFile = $state('');

  // ─── 配置状态 ───
  let cfgDbDir = $state('');
  let cfgDbEncKey = $state('');
  let cfgImageAesKey = $state('');
  let cfgImageXorKey = $state('136');
  let configPath = $state('');
  let keysInfo = $state<WechatKeysInfo | null>(null);

  // ─── HTTP API 配置 ───
  let apiEnabled = $state(true);
  let apiPort = $state('5032');
  let apiToken = $state('');
  let apiApplyResult = $state('');

  /** 生成 64 位随机十六进制令牌 */
  function generateApiToken() {
    apiToken = newApiToken();
    apiApplyResult = '已生成新令牌，保存后生效';
  }

  async function copyApiToken() {
    if (!apiToken) return;
    const ok = await copyText(apiToken);
    if (ok) {
      apiApplyResult = '令牌已复制';
      setTimeout(() => { if (apiApplyResult === '令牌已复制') apiApplyResult = ''; }, 2000);
    } else {
      apiApplyResult = '复制失败';
    }
  }

  // ─── 操作状态 ───
  let configLoading = $state(false);
  let configSaving = $state(false);
  let configError = $state('');
  let configSuccess = $state('');
  let verifyDbResult = $state('');
  let verifyLoading = $state('');
  let decrypting = $state(false);
  let decryptResult = $state('');
  let workingMessage = $state('');
  let autoRunning = $state(false);
  let autoResult = $state('');

  // ─── 检测本机微信数据（迁移自 WeChatDataAnalysis 的检测页）───
  let detecting = $state(false);
  let detectError = $state('');
  let detectedAccounts = $state<DetectedAccount[]>([]);

  async function detectWechatData() {
    detecting = true;
    detectError = '';
    try {
    const list = await detectWechatAccounts();
      detectedAccounts = Array.isArray(list) ? list : [];
      if (detectedAccounts.length === 0) {
        detectError = '未在本机检测到微信 4.x 数据（可手动选择数据库目录）';
      }
    } catch (e: unknown) {
      detectError = `检测失败: ${e}`;
      logError('detectWechatData', e);
    } finally {
      detecting = false;
    }
  }

  function useDetectedAccount(acc: DetectedAccount) {
    const dbDir = acc?.db_dir;
    if (!dbDir) return;
    cfgDbDir = dbDir;
    configSuccess = `已填入账号 ${acc.wxid || ''} 的数据库目录，请继续校验密钥`;
    setTimeout(() => { if (configSuccess.startsWith('已填入')) configSuccess = ''; }, 4000);
  }

  // ─── 一键全自动（对标 WeFlow） ───
  async function ensureDbDir(): Promise<boolean> {
    if (cfgDbDir.trim()) return true;
    try {
    const list = await detectWechatAccounts();
      if (Array.isArray(list) && list.length > 0) {
        const dir = list[0]?.db_dir;
        if (dir) {
          cfgDbDir = dir;
          return true;
        }
      }
    } catch { /* 忽略，走手动选择 */ }
    return false;
  }

  function applyAutoDbResult(r: AutoDbKeyResult) {
    if (!r) return;
    if (r.key) cfgDbEncKey = r.key;
    if (r.db_dir) cfgDbDir = r.db_dir;
    verifyDbResult = `自动获取成功：密钥 ${String(r.key || '').slice(0, 8)}…，${r.valid ?? 0}/${r.total ?? 0} 个数据库校验通过`;
    autoResult = `数据库密钥：${r.valid ?? 0}/${r.total ?? 0} 个数据库校验通过`;
    if (Array.isArray(r.errors) && r.errors.length) {
      configError = r.errors.slice(0, 2).join('; ');
    }
  }

  function applyAutoImgResult(r: AutoImgKeyResult) {
    if (!r) return;
    if (r.aes_key) cfgImageAesKey = r.aes_key;
    if (r.xor_key != null) cfgImageXorKey = String(r.xor_key);
    verifyImgResult = r.verified
      ? `自动获取：已通过模板校验 (${r.aes_key}, XOR=${r.xor_key})`
      : `自动获取：已保存但未通过模板校验 (${r.aes_key}, XOR=${r.xor_key})`;
    autoResult = (autoResult ? `${autoResult}；` : '') + `图片密钥${r.verified ? '已通过模板校验' : '已获取（未校验）'}`;
  }

  async function runAutoGetKeys(mode: 'all' | 'db' | 'img' | 'db-v2') {
    if (!(await ensureDbDir())) {
      configError = '未找到微信账号，请先点击「开始检测」或手动选择数据库目录';
      return;
    }
    if (mode === 'all' || mode === 'db') {
      // Hook 方案超时（如微信未登录）会自动回退「调试器方案」：临时关闭微信、
      // 以调试方式重启并需重新扫码登录一次。提前告知，避免微信会话被意外重启。
      const ok = confirm(
        '自动获取数据库密钥将向微信进程注入 Hook（需微信已登录运行）。\n' +
        '若 Hook 超时（如微信未登录），会自动回退「调试器方案」：' +
        '临时关闭微信并以调试方式重启，需重新扫码登录一次。\n' +
        '是否继续？',
      );
      if (!ok) return;
    }
    if (mode === 'db-v2') {
      const ok = confirm(
        '「调试器方案」需要临时关闭当前微信并以调试方式重启，' +
        '请在弹出的微信窗口完成扫码登录后等待片刻（数据库打开时自动提取密钥）。' +
        '提取完成后会自动恢复正常启动微信。是否继续？',
      );
      if (!ok) return;
    }
    configError = ''; configSuccess = ''; autoResult = '';
    autoRunning = true;
    try {
      if (mode === 'all') {
        startOp('auto_keys', '正在全自动获取密钥（数据库 + 图片）…');
    const r = await autoGetWechatKeys(180000);
        if (r?.db_key) applyAutoDbResult(r.db_key);
        if (r?.image_key) applyAutoImgResult(r.image_key);
      } else if (mode === 'db') {
        startOp('auto_db_key', '正在自动获取数据库密钥…');
    applyAutoDbResult(await autoGetDbKey(180000));
      } else if (mode === 'db-v2') {
        startOp('auto_db_key_v2', '正在以调试器方案获取数据库密钥（微信将重启）…');
        applyAutoDbResult(await autoGetDbKeyV2(300000));
      } else {
        startOp('auto_img_key', '正在自动获取图片密钥…');
    applyAutoImgResult(await autoGetImageKey());
      }
      await loadConfig();
      await loadKeysInfo();
      if (autoResult) configSuccess = autoResult;
    } catch (e: unknown) {
      configError = `自动获取失败: ${e}`;
      verifyDbResult = `${e}`;
      logError('runAutoGetKeys', e);
    } finally {
      autoRunning = false; workingMessage = ''; endOp();
    }
  }

  // ── 操作进度条（后端 wechat-op-progress 事件驱动） ──
  let opCurrent = '';
  let opProgress = $state<{ show: boolean; label: string; percent: number }>({
    show: false, label: '', percent: 0,
  });
  let opUnlisten: (() => void) | null = null;

  function startOp(op: string, label: string) {
    opCurrent = op;
    opProgress = { show: true, label, percent: 0 };
  }
  function endOp() {
    opCurrent = '';
    opProgress = { show: false, label: '', percent: 0 };
  }

  // ─── 加载配置 ───
  async function loadConfig() {
    configLoading = true; configError = '';
    try {
    const cfg = await getWechatConfig();
      configPath = cfg.configPath || '';
      const c = cfg.config || cfg.raw || {};
      cfgDbDir = c.db_dir || '';
      cfgDbEncKey = c.db_enc_key || '';
      cfgImageAesKey = c.image_aes_key || '';
      cfgImageXorKey = String(c.image_xor_key ?? 136);
      apiEnabled = c.api_enabled ?? true;
      apiPort = String(c.api_port ?? 5032);
      apiToken = c.api_token || '';
      // 固定输出目录一律取后端解析结果（相对应用目录，跨机器可移植）
      const resolved = (cfg as {
        resolved?: { decrypted_dir?: string; decoded_image_dir?: string; keys_file?: string };
      }).resolved;
      if (resolved) {
        fixedDecryptedDir = resolved.decrypted_dir || '';
        fixedDecodedImageDir = resolved.decoded_image_dir || '';
        fixedKeysFile = resolved.keys_file || '';
      }
    } catch(e: unknown) {
      configError = `加载失败: ${e}`;
      logError('loadConfig', e);
    }
    finally { configLoading = false; }
  }

  async function loadKeysInfo() {
    try {
    keysInfo = await getWechatKeysInfo();
    } catch (e) {
      keysInfo = null;
      logError('loadKeysInfo', e);
    }
  }

  // ─── 文件夹选择（仅数据库目录可选） ───
  async function pickDbDir() {
    try {
      const selected = await open({ directory: true, multiple: false, title: '选择数据库目录 (db_storage)' });
      if (typeof selected === 'string' && selected.trim()) {
        cfgDbDir = selected.trim();
      }
    } catch (e: unknown) { configError = `选择文件夹失败: ${e}`; }
  }

  // ─── 校验 ───
  async function verifyDbKey() {
    const dbDir = cfgDbDir.trim();
    const encKey = cfgDbEncKey.trim();
    if (!dbDir) { configError = '请先选择数据库目录'; return; }
    if (!encKey) { configError = '请输入 PBKDF2 口令'; return; }
    configError = ''; configSuccess = '';
    verifyLoading = 'db'; verifyDbResult = '';
    startOp('verify_db', '正在校验数据库密钥…');
    try {
      const dbPath = `${dbDir.replace(/\\+$/, '').replace(/\/+$/, '')}/session/session.db`;
      workingMessage = '正在校验密钥（PBKDF2 派生，约数秒）…';
      const r = await verifyDatabaseKey(dbPath, encKey);
      if (r.valid) {
        verifyDbResult = `通过 (${r.format})，正在生成密钥文件…`;
        workingMessage = '正在并行校验全部数据库并生成密钥映射…';
        const gen = await generateKeysFile({
          dbDir, keysFile: fixedKeysFile, encKeyHex: encKey,
          keyFormat: r.format === 'v4.0' ? null : 'wx_key_v4.1',
        });
        verifyDbResult = `通过 (${r.format})，已生成 (${gen.valid}/${gen.total} 个数据库)`;
        configSuccess = `密钥文件已生成 (${gen.valid} 个可用)`;
        await loadKeysInfo();
      } else {
        verifyDbResult = '密钥不正确';
      }
    } catch(e: unknown) {
      verifyDbResult = `${e}`;
      logError('verifyDbKey', e);
    }
    finally { verifyLoading = ''; workingMessage = ''; endOp(); }
  }

  // ─── 导入备份（账号归档 ZIP 或已解密目录）───
  let importBusy = $state(false);
  let importResult = $state('');
  // ── CDN 自动获取原图开关 ──
  let cdnEnabled = $state(true);
  let cdnLocalDecrypt = $state(true);
  // ── 账号一致性 ──
  let accountStatus = $state<WechatAccountStatus | null>(null);
  async function loadAccountStatus() {
    try {
      accountStatus = await getWechatAccountStatus();
    } catch { /* 忽略 */ }
  }
  // ── 一键切换到当前登录账号并重新获取密钥 ──
  let switchingAccount = $state(false);
  let switchAccountMsg = $state('');
  async function switchToLiveAccount() {
    if (switchingAccount) return;
    switchingAccount = true;
    switchAccountMsg = '正在切换到当前登录账号并获取密钥（可能需要 1-3 分钟）…';
    try {
      const r = await switchWechatAccountToLive(240000);
      if (r?.switched) {
        switchAccountMsg = r?.db_key_error
          ? `已切换到 ${r.live_account ?? ''}，但密钥获取失败：${r.db_key_error}`
          : `已切换到 ${r.live_account ?? ''} 并获取密钥`;
      } else {
        switchAccountMsg = '当前已是登录账号，无需切换';
      }
      if (r?.monitor_error) switchAccountMsg += `（监控重启失败：${r.monitor_error}）`;
      await Promise.all([loadConfig(), loadKeysInfo(), loadAccountStatus()]);
    } catch (e: unknown) {
      switchAccountMsg = `切换失败：${errText(e) || '未知错误'}`;
      logError('switchToLiveAccount', e);
    } finally {
      switchingAccount = false;
      setTimeout(() => {
        if (!switchingAccount) switchAccountMsg = '';
      }, 6000);
    }
  }

  // ── 本地离线语音转写（whisper.cpp）──
  let sttStatus = $state<SttStatus | null>(null);
  let sttEnabled = $state(true);
  let sttModelPath = $state('');
  let sttLanguage = $state('auto');
  let sttDlSize = $state('');
  let sttDlProgress = $state<{ percent: number; filename: string } | null>(null);
  let sttMsg = $state('');
  let sttOpUnlisten: (() => void) | null = null;

  async function loadSttStatus() {
    try {
      sttStatus = await getLocalSttStatus();
      sttEnabled = sttStatus?.enabled ?? true;
      sttModelPath = sttStatus?.model_path || '';
      sttLanguage = sttStatus?.language || 'auto';
    } catch (e) {
      logError('loadSttStatus', e);
    }
  }

  async function saveSttConfig(extra?: Record<string, unknown>) {
    try {
      sttStatus = await setLocalSttConfig({
        enabled: sttEnabled,
        model_path: sttModelPath,
        language: sttLanguage,
        translate: false,
        model_size: sttStatus?.model_size || 'base',
        ...(extra ?? {}),
      });
      sttMsg = sttStatus?.model_loaded
        ? '本地模型已加载，转写不再调用 API'
        : sttStatus?.model_exists
          ? '模型文件就绪，首次转写时自动加载'
          : '未选择模型文件（可点击下方按钮下载）';
      setTimeout(() => { if (sttMsg.startsWith('本地')) sttMsg = ''; }, 5000);
    } catch (e: unknown) {
      sttMsg = `保存失败：${errText(e)}`;
      logError('saveSttConfig', e);
    }
  }

  async function pickSttModel() {
    try {
      const sel = await open({
        multiple: false,
        title: '选择 Whisper GGML 模型（ggml-*.bin）',
        filters: [{ name: 'Whisper GGML', extensions: ['bin'] }],
      });
      if (typeof sel === 'string' && sel.trim()) {
        sttModelPath = sel.trim();
        await saveSttConfig({ model_path: sttModelPath });
      }
    } catch (e: unknown) {
      sttMsg = `选择模型失败：${errText(e)}`;
    }
  }

  async function downloadSttModel(size: string) {
    if (sttDlSize) return;
    sttDlSize = size;
    sttDlProgress = { percent: 0, filename: '' };
    sttMsg = '';
    try {
      const r = await downloadLocalSttModel(size);
      sttStatus = r?.status ?? sttStatus;
      sttModelPath = r?.path || sttModelPath;
      sttMsg = r?.model_loaded
        ? `模型下载完成并已加载（${(r.size_bytes / 1048576).toFixed(1)} MB）`
        : `模型已下载：${r?.path}`;
      if (r?.load_error) sttMsg += `（加载失败：${r.load_error}）`;
    } catch (e: unknown) {
      sttMsg = `下载失败：${errText(e)}`;
      logError('downloadSttModel', e);
    } finally {
      sttDlSize = '';
      setTimeout(() => {
        if (!sttDlSize) { sttDlProgress = null; sttMsg = ''; }
      }, 8000);
    }
  }

  async function loadCdnStatus() {
    try {
      const s = await getCdnImageStatus();
      cdnEnabled = s?.enabled !== false;
      cdnLocalDecrypt = s?.localDecrypt !== false;
    } catch { /* 忽略 */ }
  }
  async function toggleCdn() {
    cdnEnabled = !cdnEnabled;
    try {
    await setCdnImageEnabled(cdnEnabled);
      configSuccess = cdnEnabled ? '已开启自动获取原图（CDN）' : '已关闭自动获取原图（CDN）';
    } catch (e: unknown) {
      configError = `设置失败: ${e}`;
      cdnEnabled = !cdnEnabled;
    }
  }
  async function toggleCdnLocalDecrypt() {
    cdnLocalDecrypt = !cdnLocalDecrypt;
    try {
      await setCdnImageLocalDecrypt(cdnLocalDecrypt);
      configSuccess = cdnLocalDecrypt
        ? '已切换为本地解密：aeskey 不再发送给 CDN 服务'
        : '已切换为服务端解密：由 CDN 服务端解密原图';
    } catch (e: unknown) {
      configError = `设置失败: ${e}`;
      cdnLocalDecrypt = !cdnLocalDecrypt;
    }
  }
  async function doImport(source: string) {
    importBusy = true;
    importResult = '';
    configError = '';
    startOp('import_backup', '正在导入备份…');
    try {
      const r = await importWechatBackup({ source });
      importResult = `已导入 ${r.imported} 个文件 → ${r.target}`;
      configSuccess = importResult;
    } catch (e: unknown) {
      configError = `导入失败: ${e}`;
      importResult = '';
    } finally {
      importBusy = false;
      endOp();
    }
  }
  async function importBackupZip() {
    try {
      const fileSel = await open({
        multiple: false,
        filters: [{ name: '账号归档 ZIP', extensions: ['zip'] }],
        title: '选择账号归档 ZIP',
      });
      if (typeof fileSel === 'string' && fileSel.trim()) await doImport(fileSel.trim());
    } catch (e: unknown) { configError = `选择文件失败: ${e}`; }
  }
  async function importBackupDir() {
    try {
      const dirSel = await open({
        directory: true,
        multiple: false,
        title: '选择已解密的微信备份目录',
      });
      if (typeof dirSel === 'string' && dirSel.trim()) await doImport(dirSel.trim());
    } catch (e: unknown) { configError = `选择目录失败: ${e}`; }
  }

  // ─── 立即解密全部数据库 ───
  async function decryptAllDb() {
    const dbDir = cfgDbDir.trim();
    if (!dbDir) { configError = '请先选择数据库目录'; return; }
    decrypting = true; decryptResult = ''; configError = ''; configSuccess = '';
    startOp('decrypt_all', '正在解密数据库…');
    try {
      const r = await decryptAllDatabases({
        keysFile: fixedKeysFile,
        dbDir,
        decryptedDir: fixedDecryptedDir,
      });
      const base = `解密完成: ${r.decrypted}/${r.total} 个数据库`;
      const wal = r.wal_patched > 0 ? `, ${r.wal_patched} WAL 页` : '';
      if (r.errors?.length) {
        decryptResult = `${base}${wal}, ${r.errors.length} 个错误`;
        configError = r.errors.slice(0, 3).join('; ');
      } else {
        configSuccess = `${base}${wal}`;
      }
      setTimeout(() => { configSuccess = ''; decryptResult = ''; }, 6000);
    } catch (e: unknown) {
      configError = `解密失败: ${e}`;
      logError('decryptAllDb', e);
    }
    finally { decrypting = false; workingMessage = ''; }
  }

  // ─── 图片密钥校验 ───
  let verifyImgResult = $state('');
  let imgDecodeResult = $state('');
  let decodeImgLoading = $state(false);

  async function verifyImageKey() {
    const dbDir = cfgDbDir.trim();
    if (!dbDir) { configError = '请先选择数据库目录'; return; }
    configError = ''; configSuccess = '';
    verifyImgResult = '校验中…';
    startOp('verify_img', '正在校验图片密钥…');
    try {
      const r = await callVerifyImageKey({
        dbDir,
        aesKeyHex: cfgImageAesKey.trim(),
        xorKeyStr: cfgImageXorKey || '136',
      });
      if (r.valid) {
        verifyImgResult = `通过 (${r.format})，缓存 ${r.total_cached} 个文件`;
      } else {
        verifyImgResult = '密钥不正确或未找到可识别的文件';
      }
    } catch (e: unknown) {
      verifyImgResult = `校验失败: ${e}`;
      logError('verifyImageKey', e);
    } finally {
      endOp();
    }
  }

  async function decodeAllImages() {
    const dbDir = cfgDbDir.trim();
    if (!dbDir) { configError = '请先选择数据库目录'; return; }
    decodeImgLoading = true; imgDecodeResult = ''; configError = ''; configSuccess = '';
    startOp('decode_img', '正在解码图片…');
    try {
      const r = await callDecodeAllImages({
        dbDir,
        outputDir: fixedDecodedImageDir,
        aesKeyHex: cfgImageAesKey.trim(),
        xorKeyStr: cfgImageXorKey || '136',
      });
      imgDecodeResult = `解码完成: ${r.decoded}/${r.total} 个图片`;
      if (r.errors?.length) {
        configError = r.errors.slice(0, 2).join('; ');
      }
    } catch (e: unknown) {
      configError = `解码失败: ${e}`;
      logError('decodeAllImages', e);
    }
    finally { decodeImgLoading = false; endOp(); }
  }

  // ─── 保存配置 ───
  async function saveConfig() {
    configSaving = true; configError = ''; configSuccess = '';
    try {
      const payload = {
        db_dir: cfgDbDir.trim() || undefined,
        // 输出目录不再回写：留空由后端解析为 <应用目录>/data/wechat 下的默认值，
        // 避免把本机绝对路径持久化进 config.json（部署到客户电脑仍可移植）
        decrypted_dir: undefined,
        decoded_image_dir: undefined,
        keys_file: undefined,
        db_enc_key: cfgDbEncKey.trim() || undefined,
        image_aes_key: cfgImageAesKey.trim() || undefined,
        image_xor_key: cfgImageXorKey ? safeParseInt(cfgImageXorKey, 136, 0, 2147483647) : undefined,
        key_format: 'wx_key_v4.1',
        wechat_process: 'Weixin.exe',
        api_enabled: apiEnabled,
        api_port: safeParseInt(apiPort, 5032, 1024, 65535),
        api_token: apiToken.trim() || undefined,
      };
    await saveWechatConfig(payload);
      // 热应用 API 设置：令牌即时生效，端口变化时自动重启监听，无需重启应用
      try {
    await applyApiSettings();
        apiApplyResult = 'API 设置已生效';
        setTimeout(() => { if (apiApplyResult === 'API 设置已生效') apiApplyResult = ''; }, 3000);
      } catch (e) {
        apiApplyResult = '已保存，API 热应用失败（重启应用后生效）';
        logError('apply_api_settings', e);
      }
      configSuccess = '配置已保存';
      cfgImageXorKey = String(payload.image_xor_key ?? cfgImageXorKey);
      await loadKeysInfo();
      setTimeout(() => configSuccess = '', 3000);
    } catch(e: unknown) {
      configError = `保存失败: ${e}`;
      logError('saveConfig', e);
    }
    finally { configSaving = false; }
  }

  onMount(async () => {
    try {
      opUnlisten = await listen<WeChatOpProgress>('wechat-op-progress', (e) => {
        const p = e.payload;
        if (!p?.op || p.op !== opCurrent) return;
        opProgress = { show: true, label: p.message || opProgress.label, percent: p.percent ?? 0 };
      });
      sttOpUnlisten = await listen<SttDownloadProgress>('stt-download-progress', (e) => {
        sttDlProgress = e.payload;
      });
      await loadConfig();
      await loadKeysInfo();
      loadCdnStatus();
      loadAccountStatus();
      loadSttStatus();
      // 启动页即检测微信状态（WeFlow 风格：进入即展示账号/密钥概况）
      detectWechatData();
    } catch (e) {
      logError('onMount', e);
    }
  });
  onDestroy(() => {
    opUnlisten?.();
    opUnlisten = null;
    sttOpUnlisten?.();
    sttOpUnlisten = null;
  });
</script>

<div class="wc-panel">
  <!-- 头部 -->
  <header class="wc-header">
    <div class="wc-brand">
      <div class="wc-brand-icon">
        <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M8 12.5c0-1.5 2-2.5 4-2.5s4 1 4 2.5"/><circle cx="8" cy="8.5" r="1"/><circle cx="16" cy="8.5" r="1"/><path d="M17.5 12c2.5 0 4.5 1.5 4.5 3.5S20 19 17.5 19"/><path d="M6.5 12C4 12 2 13.5 2 15.5S4 19 6.5 19"/><line x1="8" y1="17" x2="12" y2="13" stroke-width="1.5"/><line x1="16" y1="17" x2="12" y2="13"/></svg>
      </div>
      <div class="wc-brand-text">
        <h2 class="wc-title">微信数据配置</h2>
        <p class="wc-subtitle">配置数据库路径与解密密钥，生成密钥映射文件</p>
      </div>
    </div>
    <div class="wc-header-status">
      {#if detecting}
        <span class="wc-status-badge wc-status-busy"><span class="wc-dot"></span>检测中…</span>
      {:else if detectedAccounts.length > 0}
        <span class="wc-status-badge wc-status-ok"><span class="wc-dot"></span>{detectedAccounts.length} 个微信账号</span>
      {:else}
        <span class="wc-status-badge wc-status-idle"><span class="wc-dot"></span>未检测到微信</span>
      {/if}
      {#if (keysInfo?.keyCount ?? 0) > 0}
        <span class="wc-status-badge wc-status-ok"><span class="wc-dot"></span>密钥就绪</span>
      {:else}
        <span class="wc-status-badge wc-status-idle"><span class="wc-dot"></span>密钥未配置</span>
      {/if}
    </div>
  </header>

  {#if configError}
    <div class="wc-message wc-error">{configError}</div>
  {/if}
  {#if configSuccess}
    <div class="wc-message wc-success">{configSuccess}</div>
  {/if}
  {#if opProgress.show}
    <div class="wc-op-progress" role="status" aria-live="polite">
      <div class="wc-op-progress-track">
        <div class="wc-op-progress-fill" style="width:{opProgress.percent}%"></div>
      </div>
      <div class="wc-op-progress-meta">
        <span class="wc-op-progress-label">{opProgress.label}</span>
        <span class="wc-op-progress-pct">{opProgress.percent}%</span>
      </div>
    </div>
  {/if}

  {#if accountStatus?.mismatch}
    <div class="wc-account-warn" role="alert">
      <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.8" class="wc-account-warn-ico"><path d="M10.3 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.7 3.86a2 2 0 0 0-3.4 0z"/><line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>
      <div class="wc-account-warn-body">
        <div class="wc-account-warn-title">数据来源账号与当前登录微信不一致</div>
        <div class="wc-account-warn-sub">
          当前分析数据来自 <b class="wc-mono">{accountStatus.analysis_account}</b>
          {accountStatus.live_account_mtime ? `（该账号最近写入 ${accountStatus.live_account_mtime}）` : ''}；
          微信当前登录账号为 <b class="wc-mono">{accountStatus.live_account}</b>{accountStatus.weixin_running ? '（运行中）' : ''}。
          一键切换将自动指向当前登录账号的数据库目录并重新获取密钥。
        </div>
        <div class="wc-account-warn-actions">
          <WechatHoverButton
            text={switchingAccount ? '切换中…' : '一键切换到当前账号并获取密钥'}
            onclick={switchToLiveAccount}
            disabled={switchingAccount}
            title="将数据库目录切换到当前登录微信账号，并自动重新获取数据库密钥"
          />
          {#if switchAccountMsg}
            <span class:wc-err={switchAccountMsg.includes('失败')} style="font-size:11.5px;color:var(--wc-muted);">
              {switchAccountMsg}
            </span>
          {/if}
        </div>
      </div>
    </div>
  {/if}

  <div class="wc-body">
    <!-- ─── 数据源 ─── -->
    <section class="wc-card">
      <div class="wc-card-hd">
        <span class="wc-card-ico">
          <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.8"><circle cx="11" cy="11" r="7"/><path d="M21 21l-4.35-4.35"/></svg>
        </span>
        <div>
          <div class="wc-card-title">数据源</div>
          <div class="wc-card-sub">微信账号检测 · 数据库目录 · 产物输出位置</div>
        </div>
      </div>
      <div class="wc-card-body">
        <div class="wc-detect-toolbar">
          <WechatHoverButton text={detecting ? '检测中…' : '检测本机微信账号'} onclick={detectWechatData} disabled={detecting} class="!px-3 !py-1 !text-xs" />
          {#if detectedAccounts.length > 0}
            <span class="wc-result wc-ok">发现 {detectedAccounts.length} 个账号</span>
          {/if}
        </div>
        {#if detectError}
          <div class="wc-result wc-err">{detectError}</div>
        {/if}
        {#if detectedAccounts.length > 0}
          <div class="wc-detect-list">
            {#each detectedAccounts as acc (acc.wxid)}
              <div class="wc-detect-item">
                <div class="wc-detect-info">
                  <div class="wc-detect-top">
                    <span class="wc-detect-wxid">{acc.wxid || '未知账号'}</span>
                    <span class="wc-detect-meta">最近活跃：{fmtLastActive(acc.last_active ?? 0)}</span>
                  </div>
                  <span class="wc-detect-path" title={acc.db_dir}>{acc.db_dir}</span>
                </div>
                <WechatHoverButton text="使用此账号" onclick={() => useDetectedAccount(acc)} class="!px-3 !py-1 !text-xs" />
              </div>
            {/each}
          </div>
        {/if}
        <div class="wc-field">
          <label class="wc-label" for="wc-db-dir">数据库目录</label>
          <div class="wc-row">
            <input id="wc-db-dir" class="wc-input wc-mono" bind:value={cfgDbDir} placeholder="留空自动检测本机微信账号" />
            <WechatHoverButton text="选择文件夹" onclick={pickDbDir} title="选择数据库目录" class="!px-3 !py-1 !text-xs" />
          </div>
        </div>
        <div class="wc-path-grid">
          <div class="wc-field">
            <span class="wc-label">解密输出</span>
            <div class="wc-fixed-path" title={fixedDecryptedDir}>{fixedDecryptedDir || '（加载中…）'}</div>
          </div>
          <div class="wc-field">
            <span class="wc-label">图片输出</span>
            <div class="wc-fixed-path" title={fixedDecodedImageDir}>{fixedDecodedImageDir || '（加载中…）'}</div>
          </div>
          <div class="wc-field">
            <span class="wc-label">密钥文件</span>
            <div class="wc-fixed-path" title={fixedKeysFile}>{fixedKeysFile || '（加载中…）'}</div>
          </div>
          <div class="wc-field">
            <span class="wc-label">微信进程</span>
            <div class="wc-fixed-path">Weixin.exe</div>
          </div>
        </div>
        <div class="wc-row-between">
          <span class="wc-label">备份导入</span>
          <div class="wc-actions">
            <WechatHoverButton text={importBusy ? '导入中…' : '导入 ZIP 备份'} onclick={importBackupZip} disabled={importBusy} title="导入账号归档 ZIP" class="!px-3 !py-1 !text-xs" />
            <WechatHoverButton text="导入解密目录" onclick={importBackupDir} disabled={importBusy} title="导入已解密的微信备份目录" class="!px-3 !py-1 !text-xs" />
          </div>
        </div>
        {#if importResult}
          <div class="wc-result wc-ok">{importResult}</div>
        {/if}
        <div class="wc-row-between">
          <span class="wc-label">自动获取原图（CDN）</span>
          <WechatHoverButton text={cdnEnabled ? '已开启' : '已关闭'} onclick={toggleCdn} title="本地无原图时从微信 CDN 自动下载" class={cdnEnabled ? 'wc-ihb-active !px-3 !py-1 !text-xs' : '!px-3 !py-1 !text-xs'} />
        </div>
        <div class="wc-row-between">
          <span class="wc-label">原图解密方式</span>
          <WechatHoverButton text={cdnLocalDecrypt ? '本地解密' : '服务端解密'} onclick={toggleCdnLocalDecrypt} title="本地解密：aeskey 不出本机；服务端解密：把 aeskey 发给 CDN 代为解密" class={cdnLocalDecrypt ? 'wc-ihb-active !px-3 !py-1 !text-xs' : '!px-3 !py-1 !text-xs'} />
        </div>
      </div>
    </section>

    <!-- ─── 解密密钥 ─── -->
    <section class="wc-card">
      <div class="wc-card-hd">
        <span class="wc-card-ico">
          <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.8"><rect x="5" y="11" width="14" height="9" rx="2"/><path d="M8 11V8a4 4 0 0 1 8 0v3"/></svg>
        </span>
        <div>
          <div class="wc-card-title">解密密钥</div>
          <div class="wc-card-sub">数据库与图片解密 · 自动获取 / 手动校验</div>
        </div>
      </div>
      <div class="wc-card-body">
        <div class="wc-auto-btns">
          <WechatHoverButton text={autoRunning ? '自动获取中…' : '一键全自动获取密钥'} onclick={() => runAutoGetKeys('all')} disabled={autoRunning} />
          <WechatHoverButton text="仅数据库" onclick={() => runAutoGetKeys('db')} disabled={autoRunning} class="!px-3 !py-1 !text-xs" />
          <WechatHoverButton text="调试器 / 4.1.10+" onclick={() => runAutoGetKeys('db-v2')} disabled={autoRunning} class="!px-3 !py-1 !text-xs" />
          <WechatHoverButton text="仅图片" onclick={() => runAutoGetKeys('img')} disabled={autoRunning} class="!px-3 !py-1 !text-xs" />
        </div>
        {#if autoResult}
          <div class="wc-result wc-ok">{autoResult}</div>
        {/if}
        <div class="wc-key-grid">
          <div class="wc-key-col">
            <div class="wc-field">
              <label class="wc-label" for="wc-pbkdf2-key">数据库密钥（PBKDF2 口令）</label>
              <input id="wc-pbkdf2-key" class="wc-input wc-mono" bind:value={cfgDbEncKey} placeholder="输入 64 位 hex 主密钥 / 口令" />
            </div>
            <div class="wc-actions">
              <WechatHoverButton text={verifyLoading === 'db' ? '校验中…' : '校验数据库密钥'} onclick={verifyDbKey} disabled={!cfgDbDir || !cfgDbEncKey || !!verifyLoading} class="!px-3 !py-1 !text-xs" />
              <WechatHoverButton text={decrypting ? '解密中…' : '立即解密数据库'} onclick={decryptAllDb} disabled={decrypting || !cfgDbDir} class="!px-3 !py-1 !text-xs" />
            </div>
            {#if verifyDbResult}
              <span class="wc-result" class:wc-ok={verifyDbResult.startsWith('通过')} class:wc-err={!verifyDbResult.startsWith('通过') && !verifyDbResult.startsWith('正在')} style={verifyDbResult.startsWith('正在') ? 'color:var(--app-color-accent)' : ''}>{verifyDbResult}</span>
            {/if}
            {#if decryptResult}
              <span class="wc-result" class:wc-ok={!decryptResult.includes('错误')} class:wc-err={decryptResult.includes('错误')}>{decryptResult}</span>
            {/if}
          </div>
          <div class="wc-key-col">
            <div class="wc-field">
              <label class="wc-label" for="wc-img-aes">图片 AES 密钥（可选）</label>
              <div class="wc-row">
                <input id="wc-img-aes" class="wc-input wc-mono" bind:value={cfgImageAesKey} placeholder="32 位 hex / 留空自动" />
                <input class="wc-input wc-num" bind:value={cfgImageXorKey} placeholder="XOR" title="XOR key（默认 136）" />
              </div>
            </div>
            <div class="wc-actions">
              <WechatHoverButton text="校验图片密钥" onclick={verifyImageKey} disabled={!cfgDbDir} class="!px-3 !py-1 !text-xs" />
              <WechatHoverButton text={decodeImgLoading ? '解码中…' : '立即解码图片'} onclick={decodeAllImages} disabled={decodeImgLoading || !cfgDbDir} class="!px-3 !py-1 !text-xs" />
            </div>
            {#if verifyImgResult}
              <span class="wc-result" class:wc-ok={verifyImgResult.startsWith('通过')} class:wc-err={verifyImgResult !== '校验中…' && !verifyImgResult.startsWith('通过')} style={verifyImgResult === '校验中…' ? 'color:var(--app-color-accent)' : ''}>{verifyImgResult}</span>
            {/if}
            {#if imgDecodeResult}
              <span class="wc-result" class:wc-ok={!imgDecodeResult.includes('错误')} class:wc-err={imgDecodeResult.includes('错误')}>{imgDecodeResult}</span>
            {/if}
          </div>
        </div>
        <div class="wc-overview">
          <div class="wc-overview-hd">密钥文件概览</div>
          {#if keysInfo}
            <div class="wc-overview-stats">
              <div class="wc-stat"><span class="wc-stat-val">{keysInfo.keyFormat || '-'}</span><span class="wc-stat-label">格式</span></div>
              <div class="wc-stat"><span class="wc-stat-val">{keysInfo.keyCount ?? 0}</span><span class="wc-stat-label">密钥数</span></div>
            </div>
          {:else}
            <p class="wc-hint">尚未加载密钥文件</p>
          {/if}
        </div>
        <p class="wc-hint">一键自动完成：检测微信进程 → 注入密钥钩子 → 生成 all_keys.json → 获取图片密钥并写入配置（微信需已启动并登录）；微信 4.1.10.31+ 请使用「调试器」方案。</p>
      </div>
    </section>

    <!-- ─── HTTP API 服务 ─── -->
    <section class="wc-card">
      <div class="wc-card-head">
        <div>
          <div class="wc-card-title">HTTP API 服务</div>
          <div class="wc-card-sub">微信数据只读接口与 SSE 实时推送（仅监听 127.0.0.1）</div>
        </div>
        <label class="wc-api-switch" title="启用/停用 HTTP API 服务">
          <input type="checkbox" bind:checked={apiEnabled} />
          <span class:wc-api-on={apiEnabled} class:wc-api-off={!apiEnabled}>{apiEnabled ? '已启用' : '已停用'}</span>
        </label>
      </div>
      <div class="wc-card-body">
        <div class="wc-field">
          <label class="wc-label" for="wc-api-token">访问令牌 api_token（留空 = 免鉴权）</label>
          <div class="wc-row">
            <input id="wc-api-token" class="wc-input wc-mono" bind:value={apiToken} placeholder="留空则免鉴权" spellcheck="false" />
          </div>
        </div>
        <div class="wc-actions">
          <WechatHoverButton text="生成随机令牌" onclick={generateApiToken} class="!px-3 !py-1 !text-xs" />
          <WechatHoverButton text="复制令牌" onclick={copyApiToken} disabled={!apiToken} class="!px-3 !py-1 !text-xs" />
          {#if apiApplyResult}
            <span class="wc-result" class:wc-ok={!apiApplyResult.includes('失败')} class:wc-err={apiApplyResult.includes('失败')}>{apiApplyResult}</span>
          {/if}
        </div>
        <div class="wc-field">
          <label class="wc-label" for="wc-api-port">监听端口</label>
          <input id="wc-api-port" class="wc-input wc-num" bind:value={apiPort} placeholder="5032" />
        </div>
        <p class="wc-hint">
          服务地址 <code>http://127.0.0.1:{safeParseInt(apiPort, 5032, 1024, 65535)}</code>
          ，保存后自动生效；接口用法见左侧「API 文档」。
        </p>
      </div>
    </section>

    <!-- ─── 本地语音转写（离线） ─── -->
    <section class="wc-card">
      <div class="wc-card-head">
        <div>
          <div class="wc-card-title">本地语音转写（离线）</div>
          <div class="wc-card-sub">whisper.cpp 开源引擎 · 无需联网/API · 99 种语言</div>
        </div>
        <label class="wc-api-switch" title="启用后聊天「转文字」优先本地识别">
          <input type="checkbox" bind:checked={sttEnabled} onchange={() => saveSttConfig()} />
          <span class:wc-api-on={sttEnabled} class:wc-api-off={!sttEnabled}>{sttEnabled ? '已启用' : '已停用'}</span>
        </label>
      </div>
      <div class="wc-card-body">
        <div class="wc-field">
          <label class="wc-label" for="wc-stt-model">Whisper 模型（ggml-*.bin）</label>
          <div class="wc-row">
            <input id="wc-stt-model" class="wc-input wc-mono" bind:value={sttModelPath} placeholder="选择或下载 ggml-base.bin" />
            <WechatHoverButton text="选择文件" onclick={pickSttModel} class="!px-3 !py-1 !text-xs" />
          </div>
        </div>
        <div class="wc-actions">
          {#each (sttStatus?.available_models ?? []) as m}
            <WechatHoverButton text={`下载 ${m.label}`} onclick={() => downloadSttModel(m.value)} disabled={!!sttDlSize} class="!px-3 !py-1 !text-xs" />
          {/each}
        </div>
        {#if sttDlProgress}
          <div class="wc-stt-dl">
            <div class="wc-stt-dl-bar"><span style="width:{sttDlProgress.percent ?? 0}%"></span></div>
            <span class="wc-stt-dl-text wc-mono">{sttDlProgress.percent ?? 0}% {sttDlProgress.filename || ''}</span>
          </div>
        {/if}
        {#if sttMsg}
          <div class="wc-result" class:wc-ok={!sttMsg.includes('失败') && !sttMsg.includes('未')} class:wc-err={sttMsg.includes('失败')}>{sttMsg}</div>
        {/if}
        <div class="wc-field">
          <label class="wc-label" for="wc-stt-lang">识别语言</label>
          <select id="wc-stt-lang" class="wc-input" bind:value={sttLanguage} onchange={() => saveSttConfig()}>
            {#each (sttStatus?.languages ?? []) as l}
              <option value={l.value}>{l.label}</option>
            {/each}
          </select>
        </div>
        <div class="wc-overview">
          <div class="wc-overview-hd">模型状态</div>
          <div class="wc-overview-stats">
            <div class="wc-stat"><span class="wc-stat-val">{sttStatus?.model_loaded ? '已加载' : sttStatus?.model_exists ? '就绪' : '未配置'}</span><span class="wc-stat-label">状态</span></div>
            <div class="wc-stat"><span class="wc-stat-val">{sttStatus?.model_size_bytes ? `${(sttStatus.model_size_bytes / 1048576).toFixed(1)} MB` : '-'}</span><span class="wc-stat-label">大小</span></div>
          </div>
        </div>
        <p class="wc-hint">启用后聊天「转文字」优先本地识别，不消耗 API 额度；模型加载后常驻内存。</p>
      </div>
    </section>
  </div>

  <!-- 操作栏 -->
  <div class="wc-actions-bar">
    <div class="wc-actions-meta">
      {#if configPath}
        <span class="wc-meta-path" title={configPath}>配置文件：{configPath}</span>
      {/if}
    </div>
    <div class="wc-actions-btns">
        <WechatHoverButton text={configSaving ? '保存中…' : '保存配置'} onclick={saveConfig} disabled={configSaving || configLoading} />
    </div>
  </div>

  {#if workingMessage}
    <div class="wc-overlay">
      <div class="wc-overlay-box">
        <div class="wc-spinner"></div>
        <p class="wc-overlay-text">{workingMessage}</p>
      </div>
    </div>
  {/if}
</div>

<style>
  .wc-panel { display: flex; flex-direction: column; gap: 14px; position: relative; min-height: 300px; }

  /* ─── 头部 ─── */
  .wc-header { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
  .wc-brand { display: flex; align-items: center; gap: 12px; }
  .wc-header-status { display: flex; align-items: center; gap: 8px; flex-shrink: 0; }
  .wc-status-badge {
    display: inline-flex; align-items: center; gap: 6px;
    font-size: 11.5px; font-weight: 600; padding: 5px 10px;
    border-radius: 999px; border: 1px solid var(--app-color-border);
    background: var(--app-color-surface-alt); color: var(--app-color-muted);
    white-space: nowrap;
  }
  .wc-status-badge .wc-dot { width: 7px; height: 7px; border-radius: 50%; background: var(--app-color-muted); }
  .wc-status-ok { color: var(--app-success, #15803d); border-color: color-mix(in srgb, var(--app-success, #22c55e) 22%, transparent); background: color-mix(in srgb, var(--app-success, #22c55e) 10%, transparent); }
  .wc-status-ok .wc-dot { background: var(--app-success, #4ade80); box-shadow: 0 0 0 3px color-mix(in srgb, var(--app-success, #22c55e) 15%, transparent); }
  .wc-status-idle .wc-dot { background: var(--app-color-muted); }
  .wc-status-busy .wc-dot { background: var(--app-warning, #fbbf24); animation: wc-blink 1s ease-in-out infinite; }
  @keyframes wc-blink { 50% { opacity: 0.25; } }
  .wc-brand-icon {
    width: 42px; height: 42px; border-radius: 12px; flex-shrink: 0;
    background: var(--app-wc-accent, #576b95); color: #fff;
    display: flex; align-items: center; justify-content: center;
    box-shadow: 0 4px 14px color-mix(in srgb, var(--app-wc-accent, #576b95) 30%, transparent);
  }
  .wc-title { font-size: 16px; font-weight: 700; margin: 0; line-height: 1.25; }
  .wc-subtitle { font-size: 11.5px; color: var(--app-color-muted); margin: 2px 0 0; }
  .wc-op-progress { margin: 10px 16px 0; padding: 10px 14px; border: 1px solid var(--app-color-border); border-radius: 10px; background: var(--app-color-card-bg); }
  .wc-op-progress-track { height: 8px; border-radius: 4px; background: color-mix(in srgb, var(--app-color-accent) 18%, transparent); overflow: hidden; }
  .wc-op-progress-fill { height: 100%; border-radius: 4px; background: var(--app-color-accent); transition: width .2s ease; }
  .wc-op-progress-meta { display: flex; justify-content: space-between; align-items: center; margin-top: 6px; font-size: 12px; color: var(--app-color-text); }
  .wc-op-progress-label { color: var(--app-color-muted); }
  .wc-op-progress-pct { font-variant-numeric: tabular-nums; font-weight: 600; color: var(--app-color-accent); }

  /* ─── 消息 ─── */
  .wc-message { font-size: 12px; padding: 8px 12px; border-radius: 8px; }
  .wc-error { background: #ef44441a; color: #f87171; border: 1px solid #ef444433; }
  .wc-success { background: #22c55e1a; color: #15803d; border: 1px solid #22c55e33; }
  /* 账号不一致警示 */
  .wc-account-warn {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    padding: 10px 14px;
    margin-bottom: 12px;
    border-radius: 10px;
    background: color-mix(in srgb, #f5a623 12%, transparent);
    border: 1px solid color-mix(in srgb, #f5a623 42%, transparent);
    color: color-mix(in srgb, #b9770c 85%, #000);
  }
  .wc-account-warn-ico { flex-shrink: 0; margin-top: 1px; }
  .wc-account-warn-body { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .wc-account-warn-title { font-size: 13px; font-weight: 700; }
  .wc-account-warn-sub { font-size: 11.5px; line-height: 1.65; opacity: 0.92; word-break: break-all; }
  .wc-account-warn-sub b { font-weight: 700; }
  .wc-account-warn-actions { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; margin-top: 4px; }
  .wc-stt-dl { margin-top: 8px; display: flex; flex-direction: column; gap: 5px; }
  .wc-stt-dl-bar { height: 4px; border-radius: 9999px; background: var(--wc-border-light); overflow: hidden; }
  .wc-stt-dl-bar span { display: block; height: 100%; background: var(--wc-theme); transition: width .2s ease; }
  .wc-stt-dl-text { font-size: 10.5px; color: var(--wc-muted); }

  /* ─── 网格 ─── */
  .wc-body {
  /* 2 列主布局：数据源/密钥 为主列，API/转写 为侧列；自然高度、顶部对齐，避免旧版
     auto-fit 三列造成的卡片高度悬殊与大段空白 */
  display: grid;
  grid-template-columns: minmax(0, 1.08fr) minmax(0, 1fr);
  gap: 10px;
  align-items: stretch;
  min-height: 0;
  overflow-y: auto;
  padding: 2px;
}
/* 固定输出位置：两列紧凑展示只读路径 */
.wc-path-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px 10px; }
.wc-detect-toolbar { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.wc-auto-btns { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }

  /* ─── 卡片 ─── */
  .wc-card {
    background: var(--app-color-card-bg); border: 1px solid var(--app-color-border);
    border-radius: var(--app-radius-lg, 12px); padding: 12px 14px;
    display: flex; flex-direction: column; gap: 12px;
  }
  .wc-card-hd { display: flex; align-items: center; gap: 10px; }
  .wc-card-ico {
    width: 30px; height: 30px; border-radius: 8px; flex-shrink: 0;
    background: var(--app-color-surface-alt); color: var(--app-color-accent);
    display: flex; align-items: center; justify-content: center;
  }
  .wc-card-title { font-size: 13px; font-weight: 700; }
  .wc-card-sub { font-size: 11.5px; color: var(--app-color-muted); margin-top: 1px; }
  .wc-card-body { display: flex; flex-direction: column; gap: 8px; flex: 1; }
  /* 卡片等高时，末尾提示/结果锚定到底部，避免列内出现悬空空白 */
  .wc-card-body > .wc-hint:last-child,
  .wc-card-body > .wc-overview:last-child { margin-top: auto; }

  /* ─── 字段 ─── */
  .wc-field { display: flex; flex-direction: column; gap: 3px; }
  .wc-label { font-size: 11.5px; font-weight: 600; color: var(--app-color-muted); }
  .wc-input {
    padding: 6px 9px; border-radius: 7px; border: 1px solid var(--app-color-border);
    background: var(--app-color-surface); color: var(--app-color-text);
    font-size: 12px; outline: none; width: 100%; box-sizing: border-box;
    transition: border-color 0.15s, box-shadow 0.15s;
  }
  .wc-input:focus { border-color: var(--app-color-accent); box-shadow: 0 0 0 3px var(--app-accent-badge); }
  .wc-mono { font-family: 'Cascadia Code', 'Fira Code', monospace; font-size: 11.5px; }
  .wc-num { width: 64px; flex-shrink: 0; }
  .wc-fixed-path {
    font-family: 'Cascadia Code', 'Fira Code', monospace; font-size: 11.5px;
    padding: 4px 8px; border-radius: 6px;
    border: 1px dashed var(--app-color-border);
    background: var(--app-color-surface-alt); color: var(--app-color-muted);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis; line-height: 1.3;
    opacity: 0.85;
  }
  .wc-row { display: flex; gap: 6px; align-items: center; width: 100%; }
  .wc-row .wc-input { flex: 1; min-width: 0; }

  /* ─── 按钮 ─── */
  .wc-actions { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  /* 按钮默认左对齐；如需右推请在具体容器上加 margin-left:auto */

  /* ─── HTTP API 开关 ─── */
  .wc-card-head { display: flex; align-items: center; justify-content: space-between; gap: 10px; }
  .wc-hint { font-size: 11.5px; color: var(--app-color-muted); line-height: 1.6; margin: 0; }
  .wc-hint code { font-family: 'Cascadia Code', monospace; color: var(--app-color-accent); }
  .wc-api-switch { display: inline-flex; align-items: center; gap: 7px; cursor: pointer; flex-shrink: 0; }
  .wc-api-switch input { width: 15px; height: 15px; accent-color: var(--app-color-accent); cursor: pointer; }
  .wc-api-switch span { font-size: 12px; font-weight: 600; }
  .wc-api-on { color: #4ade80; }
  .wc-api-off { color: var(--app-color-muted); }

  /* ─── 结果 ─── */
  .wc-result { font-size: 11.5px; font-weight: 600; }
  .wc-ok { color: #4ade80; }
  .wc-err { color: #f87171; }

  /* ─── 密钥网格 ─── */
  .wc-key-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); gap: 14px; }
  .wc-key-col { display: flex; flex-direction: column; gap: 10px; }

  /* ─── 概览 ─── */
  .wc-overview { display: flex; align-items: center; gap: 10px; background: var(--app-color-surface); border: 1px solid var(--app-color-border); border-radius: 9px; padding: 6px 10px; }
  .wc-overview-hd { font-size: 11.5px; font-weight: 600; color: var(--app-color-muted); }
  .wc-overview-stats { flex: 1; display: flex; gap: 12px; }
  .wc-stat { flex: 1; display: flex; align-items: center; gap: 5px; padding: 2px 4px; min-width: 0; }
  .wc-stat-val { font-size: 13px; font-weight: 700; color: var(--app-color-accent); white-space: nowrap; }
  .wc-stat-label { font-size: 10.5px; color: var(--app-color-muted); white-space: nowrap; }

  /* ─── 操作栏 ─── */
  .wc-actions-bar {
    display: flex; align-items: center; justify-content: space-between; gap: 12px;
    padding: 12px 0 2px; border-top: 1px solid var(--app-color-border); margin-top: auto;
  }
  .wc-meta-path { font-size: 11.5px; color: var(--app-color-very-muted); font-family: monospace; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 60%; }
  .wc-actions-btns { display: flex; gap: 8px; flex-shrink: 0; }

  /* ─── Loading 遮罩 ─── */
  .wc-overlay {
    position: absolute; inset: 0; z-index: 50;
    background: rgba(0,0,0,0.35);
    display: flex; align-items: center; justify-content: center;
    border-radius: var(--app-radius-2xl, 12px);
  }
  .wc-overlay-box {
    background: var(--app-color-card-bg); border: 1px solid var(--app-color-border);
    border-radius: 14px; padding: 32px 40px;
    display: flex; flex-direction: column; align-items: center; gap: 16px;
    box-shadow: 0 12px 40px rgba(0,0,0,0.25);
  }
  .wc-spinner {
    width: 36px; height: 36px; border-radius: 50%;
    border: 3px solid var(--app-color-border);
    border-top-color: var(--app-color-accent);
    animation: wc-spin 0.8s linear infinite;
  }
  @keyframes wc-spin { to { transform: rotate(360deg); } }
  .wc-overlay-text { font-size: 13px; font-weight: 600; margin: 0; color: var(--app-color-text); }

  /* ─── 检测本机微信数据 ─── */
  .wc-detect-list { display: flex; flex-direction: column; gap: 8px; }
  .wc-detect-item {
    display: flex; align-items: center; gap: 12px; justify-content: space-between;
    padding: 9px 12px; border: 1px solid var(--app-color-border); border-radius: 9px;
    background: var(--app-color-surface);
  }
  .wc-detect-item { align-items: center; gap: 8px; padding: 6px 9px; }
  .wc-detect-top { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .wc-detect-meta { font-size: 10.5px; }
  .wc-detect-path { font-size: 10.5px; }
  .wc-stat { padding: 5px 6px; }
  .wc-stat-val { font-size: 14px; }
  .wc-detect-info { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .wc-detect-wxid { font-size: 12.5px; font-weight: 700; color: var(--app-color-text); }
  .wc-detect-path {
    font-family: 'Cascadia Code', 'Fira Code', monospace; font-size: 11.5px; color: var(--app-color-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 520px;
  }
  .wc-detect-meta { font-size: 11.5px; color: var(--app-color-very-muted); }

  @media (max-width: 680px) {
    .wc-actions-bar { flex-direction: column; align-items: stretch; }
    .wc-actions-btns { justify-content: flex-end; }
  }
</style>

