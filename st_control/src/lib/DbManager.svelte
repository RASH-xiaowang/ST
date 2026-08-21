<script lang="ts">
  import { onMount } from 'svelte';
  import { dbApi } from './db/services/ipc';
  import type {
  DbCellValue,
  DbCleanupResult,
  DbColumn,
  DbEvent,
  DbFileEntry,
  DbHeaderInfo,
  DbInfo,
  DbIntegrityResult,
  DbRow,
  DbSqlResult,
  DbTableData,
  DbTableDetail,
  DbTableStats,
} from './db/types';
  import {
    blobDataUrl,
    blobExt,
    csvEscape,
    fmtBytes,
    fmtTsValue,
    groupDbFilesByRoot,
    groupDbTables,
    isBlobPreview,
    measureTextWidth,
    utf8ToBase64,
  } from './db/dbUtils';
  import { colWidthKey, dbWidthKeyFromPath, parseColWidths } from './db/colWidths';
  import { copyText } from './clipboard';
  import { Button } from './components/ui/button';
  import { RippleButton } from 'fancy-ui-svelte';
  import { Input } from './components/ui/input';
  import { Textarea } from './components/ui/textarea';
  import { Badge } from './components/ui/badge';
  import { Checkbox } from './components/ui/checkbox';
  import { Tabs, TabsList, TabsTrigger } from './components/ui/tabs';
  import { Sheet as SheetRoot, SheetContent, SheetDescription, SheetHeader, SheetTitle } from './components/ui/sheet';
  import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from './components/ui/dropdown-menu';
  import { Dialog as DialogRoot, DialogContent, DialogFooter, DialogHeader, DialogTitle } from './components/ui/dialog';
  import { Root as SelectRoot } from './components/ui/select';
  import {
    SelectContent,
    SelectGroup,
    SelectItem,
    SelectLabel,
    SelectTrigger,
  } from './components/ui/select';
  import DatabaseIcon from '@lucide/svelte/icons/database';
  import FolderPlusIcon from '@lucide/svelte/icons/folder-plus';
  import FilePlusIcon from '@lucide/svelte/icons/file-plus';
  import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
  import SaveIcon from '@lucide/svelte/icons/save';
  import GitCompareArrowsIcon from '@lucide/svelte/icons/git-compare-arrows';
  import SearchIcon from '@lucide/svelte/icons/search';
  import XIcon from '@lucide/svelte/icons/x';
  import Columns3Icon from '@lucide/svelte/icons/columns-3';
  import LockIcon from '@lucide/svelte/icons/lock';
  import Table2Icon from '@lucide/svelte/icons/table-2';
  import CogIcon from '@lucide/svelte/icons/cog';
  import FileTextIcon from '@lucide/svelte/icons/file-text';
  import ClipboardCopyIcon from '@lucide/svelte/icons/clipboard-copy';
  import StarIcon from '@lucide/svelte/icons/star';
  import LoaderCircleIcon from '@lucide/svelte/icons/loader-circle';
  import Clock3Icon from '@lucide/svelte/icons/clock-3';
  import CopyIcon from '@lucide/svelte/icons/copy';
  import EyeIcon from '@lucide/svelte/icons/eye';
  import PencilIcon from '@lucide/svelte/icons/pencil';
  import Trash2Icon from '@lucide/svelte/icons/trash-2';
  import CheckIcon from '@lucide/svelte/icons/check';
  import XCircleIcon from '@lucide/svelte/icons/x-circle';
  import HashIcon from '@lucide/svelte/icons/hash';
  import BarChart3Icon from '@lucide/svelte/icons/bar-chart-3';
  import CheckSquareIcon from '@lucide/svelte/icons/check-square';
  import DownloadIcon from '@lucide/svelte/icons/download';
  import PlusIcon from '@lucide/svelte/icons/plus';

  // ── 属性 ──
  let { active = false, notify = () => {} }: {
    active: boolean;
    notify: (title: string, message: string, type: 'success' | 'warn' | 'error') => void;
  } = $props();
  // ---------- 数据库浏览 (CRUD) ----------
  // 内部数据库 (control.db) 相关
  let dbTables = $state<string[]>([]);
  let dbCurTable = $state('');
  // $state.raw：表数据是整块替换的只读负载，无需深代理；
  // 对几百行 × 几十列的大结果集可显著降低赋值与渲染开销
  let dbTableData = $state.raw<DbTableData | null>(null);
  let dbTableError = $state('');
  let dbTotal = $state(0); // 匹配过滤条件的总行数（首页重算，翻页沿用缓存）
  let dbPage = $state(0); // 当前页（0 起）
  let dbNextCursor = $state<string | null>(null); // keyset 下一页游标
  let dbPrevCursor = $state<string | null>(null); // keyset 上一页游标

  let dbPageSize = $state(50);
  // 排序状态
  let dbSortCol = $state('');
  let dbSortDir = $state<'asc'|'desc'>('desc');
  // 列显隐
  let dbVisibleCols = $state<Set<string> | null>(null); // null = 全部显示
  let dbColSelectorOpen = $state(false);
  // UI 子标签
  let dbSubTab = $state<'browse'|'schema'|'status'|'sql'>('browse');
  // 数据筛选（提交后端参数化 LIKE，全表过滤）
  let dbFilterText = $state('');
  let dbFilterDebounce: ReturnType<typeof setTimeout> | undefined = undefined;
  // SQL 查询
  // SQL 查询已移除
  // 表结构缓存
  let dbSchemaInfo = $state<DbColumn[]>([]);
  let dbSchemaLoading = $state(false);
  // 状态概览
  let dbStatusInfo = $state<{ tableCount: number; dbSize: number; dbPath: string } | null>(null);
  // 记录详情弹窗
  let dbDetailRow = $state<Record<string, unknown> | null>(null);
  let dbDetailRowId = $state(0); // 当前详情行的 rowid（用于读取原始 BLOB）
  let blobViewer = $state<{ column: string; rowid: number; data: DbCellValue | null } | null>(null); // 原始内容查看器
  let blobLoading = $state(false);
  let blobTab = $state<'preview' | 'hex'>('preview');
  let dbLoadRetry = $state(0); // 当前表加载失败自动重试计数
  // ── CRUD（仅内置数据库可写） ──
  let crudMode = $state<'insert' | 'edit' | null>(null);
  let crudValues = $state<Record<string, string>>({});
  let crudNulls = $state<Record<string, boolean>>({});
  let crudSaving = $state(false);
  let crudError = $state('');
  let deleteTarget = $state<{ rowid: number } | null>(null);
  let deleteSaving = $state(false);
  // 列宽拖拽
  let dbColWidths = $state<Record<string, number>>({}); // key: "table:col" → px
  let dbResizing = $state<{ col: string; startX: number; startW: number } | null>(null);
  let dbWidthsLoaded = $state(false);
  let dbAutoWidths = $state<Record<string, number>>({}); // 自动计算列宽（基于表头+前N行内容测量）
  // 字段类型 tooltip
  let dbColTip = $state<{ name: string; type: string; x: number; y: number } | null>(null);

  // 外部数据库相关：扫描目录列表（递归收集所有 .db 文件）；默认目录由后端
  // get_app_data_dirs 提供（统一 data 目录 + 旧目录兼容），不再前端硬编码路径
  let dbScanDirs = $state<string[]>([]);
  /** 后端提供的默认扫描目录（不随配置持久化） */
  let defaultScanDirs = $state<string[]>([]);
  let extDbFiles = $state<DbFileEntry[]>([]);
  /** 应用自管理的业务数据库（每日总结/消息编辑等）快捷入口 */
  let appDbs = $state<Array<{ key: string; label: string; name: string; path: string; size_bytes: number }>>([]);
  let extDbSelectedPath = $state<string | null>(null); // null = 使用内部 control.db
  let extDbSelectedName = $state('control.db');
  let extDbLoading = $state(false);
  /** 是否内置数据库（外部库只读，禁止写入） */
  const isInternalDb = $derived(!extDbSelectedPath);
  /** 可编辑列：排除 rowid 与 BLOB（二进制无法在文本表单中编辑） */
  const crudColumns = $derived.by(() => {
    return (dbTableData?.columns ?? []).filter((c: DbColumn) =>
      c.name !== 'rowid' && !String(c.col_type || '').toUpperCase().includes('BLOB')
    );
  });
  function showExtDbResult(ok: boolean, msg: string) {
    notify('数据库', msg, ok ? 'success' : 'error');
  }

  /** 切换数据子标签 */
  function switchDbTab(tab: 'browse'|'schema'|'status'|'sql') {
    dbSubTab = tab;
    if (tab === 'schema' && dbCurTable) { refreshDbSchema(); loadTableDetail(); }
    if (tab === 'status') refreshDbStatus();
    if (tab === 'status' && !extDbSelectedPath) { refreshInternalInfo(); loadEvents(); }
  }

  /** 筛选输入防抖：文本变化后 300ms 重新按后端过滤查询（回到第一页并重算总数） */
  $effect(() => {
    void dbFilterText; // 依赖：输入变化时重新调度防抖
    clearTimeout(dbFilterDebounce);
    dbFilterDebounce = setTimeout(() => {
      if (dbCurTable) loadDbTableData(dbCurTable, 0);
    }, 300);
  });

  /** 刷新表结构 */
  async function refreshDbSchema() {
    if (!dbCurTable) return;
    dbSchemaLoading = true;
    try {
      if (extDbSelectedPath) {
    dbSchemaInfo = await dbApi.externalTableSchema(extDbSelectedPath, dbCurTable);
      } else {
    dbSchemaInfo = await dbApi.tableSchema(dbCurTable);
      }
    } catch (e) {
      dbSchemaInfo = [];
      showExtDbResult(false, `读取表结构失败: ${e}`);
    } finally { dbSchemaLoading = false; }
  }

  /** 刷新状态概览 */
  async function refreshDbStatus() {
    try {
      const path = extDbSelectedPath ?? '';
      let size = 0;
      if (path) {
      const info = await dbApi.checkDbHeader(path);
        size = info.size_bytes;
      }
      dbStatusInfo = { tableCount: dbTables.length, dbSize: size, dbPath: path || '(内部数据库)' };
    } catch { dbStatusInfo = { tableCount: dbTables.length, dbSize: 0, dbPath: '' }; }
  }

  // SQL 查询函数已移除

  /** 读取配置中的扫描目录；无论持久化内容为何，都合并后端统一 data 目录 */
  async function initDbScanDirs() {
    let persisted: string[] = [];
    try {
    const items = await dbApi.getDbConfig();
      const raw = items.find(i => i.key === 'ext_db_dirs')?.value;
      if (raw) {
        const parsed = JSON.parse(raw);
        if (Array.isArray(parsed)) {
          persisted = parsed.filter((s): s is string => typeof s === 'string' && s.trim().length > 0);
        }
      }
    } catch { /* 配置不可用时使用默认 */ }
    // 剔除旧版默认目录残影：统一目录方案（J-15）前持久化的
    // %APPDATA%\st_result / st-control / st_wechat 已不存在，保留只会刷错误
    persisted = persisted.filter((d) => {
      const lower = d.toLowerCase();
      const isLegacyAppData = lower.includes('appdata') &&
        ['st_result', 'st-control', 'st_wechat'].some((k) =>
          lower.includes(`\\${k}`) || lower.includes(`/${k}`) || lower.endsWith(k));
      return !isLegacyAppData;
    });
    // 默认目录：后端 get_app_data_dirs（应用 data 目录 + 微信 data 目录 + 旧目录兼容）
    let defaults: string[] = [];
    try {
      const dirs = await dbApi.getAppDataDirs();
      defaults = (dirs?.scanDirs ?? []).filter(
        (s): s is string => typeof s === 'string' && s.trim().length > 0
      );
    } catch {
      defaults = [];
    }
    defaultScanDirs = defaults;
    // 合并去重：统一目录始终在前并保证被扫描，用户新增目录保留
    const merged = [...defaults];
    for (const d of persisted) {
      if (!merged.includes(d)) merged.push(d);
    }
    dbScanDirs = merged;
  }

  /** 持久化扫描目录配置（只存用户新增目录，默认目录不落库，保持可移植） */
  async function saveDbScanDirs() {
    try {
      const extras = dbScanDirs.filter((d) => !defaultScanDirs.includes(d));
      await dbApi.setDbConfig('ext_db_dirs', JSON.stringify(extras));
    } catch {}
  }

  /** 添加一个扫描目录（配置持久化） */
  async function addDbScanDir() {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const selected = await open({ directory: true, multiple: false, title: '选择要扫描的数据库目录' });
      if (typeof selected === 'string' && selected.trim()) {
        const dir = selected.trim();
        if (!dbScanDirs.includes(dir)) {
          dbScanDirs = [...dbScanDirs, dir];
          await saveDbScanDirs();
          await refreshExtDbFiles();
          notify('数据库', `已添加目录: ${dir}`, 'success');
        }
      }
    } catch (e) {
      console.warn('添加扫描目录失败:', e);
    }
  }

  /** 打开任意 SQLite 数据库文件（自动把所在目录加入扫描列表并持久化，进入白名单） */
  async function openDbFile() {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const selected = await open({
        multiple: false,
        title: '打开数据库文件',
        filters: [{ name: '数据库文件', extensions: ['db', 'sqlite', 'sqlite3', 'db3', 'sdb'] }],
      });
      if (typeof selected !== 'string' || !selected.trim()) return;
      const filePath = selected.trim();
      const idx = Math.max(filePath.lastIndexOf('\\'), filePath.lastIndexOf('/'));
      const dir = idx > 0 ? filePath.slice(0, idx) : '';
      if (dir && !dbScanDirs.includes(dir)) {
        dbScanDirs = [...dbScanDirs, dir];
        await saveDbScanDirs();
      }
      await refreshExtDbFiles();
      const name = filePath.split(/[\\/]/).pop() || filePath;
      await selectExternalDb(filePath, name);
      notify('数据库', `已打开: ${name}`, 'success');
    } catch (e) {
      console.warn('打开数据库文件失败:', e);
      showExtDbResult(false, `打开失败: ${e}`);
    }
  }

  /** 扫描多个目录下的 .db 文件，合并去重后展示（单个目录失败不影响其它目录） */
  async function refreshExtDbFiles() {
    extDbLoading = true;
    try {
      try {
    appDbs = await dbApi.listAppDatabases();
      } catch { /* 后端未提供时忽略 */ }
      // 内部 control.db 的绝对路径（用于去重，避免与内置数据源按钮重复）
      let internalPath = '';
      try {
    const info = await dbApi.getDbInfo();
        internalPath = info.path.replace(/\\/g, '/').toLowerCase();
      } catch { /* 忽略，内部路径不可用时不去重 */ }

      const merged = new Map<string, DbFileEntry>();
      let failedDirs = 0;
      for (const dir of dbScanDirs) {
        try {
    const files = await dbApi.scanExternalDbs(dir);
          for (const f of files) {
            const norm = f.path.replace(/\\/g, '/').toLowerCase();
            if (internalPath && norm === internalPath) continue;
            merged.set(f.path, f);
          }
        } catch (e) {
          failedDirs += 1;
          console.warn(`扫描目录失败: ${dir}`, e);
        }
      }
      if (failedDirs > 0) {
        notify('数据库', `${failedDirs} 个扫描目录不可用，已跳过`, 'warn');
      }
      extDbFiles = Array.from(merged.values()).sort((a, b) => b.size_bytes - a.size_bytes);
    } finally {
      extDbLoading = false;
    }
  }

  /** 选中一个外部数据库文件 */
  async function selectExternalDb(path: string, name: string) {
    extDbSelectedPath = path;
    extDbSelectedName = name;
    dbCurTable = '';
    dbTableData = null;
    dbTableSearch = ''; // 切换数据源时清空表搜索，避免误以为没有表
    await loadExtDbTables();
  }

  /** 切换回内部数据库 */
  async function selectInternalDb() {
    extDbSelectedPath = null;
    extDbSelectedName = 'control.db';
    dbCurTable = '';
    dbTableData = null;
    dbTableSearch = ''; // 切换数据源时清空表搜索
    await loadDbTables();
  }

  /** 加载表列表（根据当前模式自动选择） */
  async function loadDbTables() {
    if (extDbSelectedPath) {
      await loadExtDbTables();
      return;
    }
    try { dbTables = await dbApi.listTables(); } catch {}
  }

  /** 加载外部数据库的表列表 */
  async function loadExtDbTables() {
    if (!extDbSelectedPath) return;
    try {
    dbTables = await dbApi.externalListTables(extDbSelectedPath);
    } catch (e) {
      dbTables = [];
      // 自动触发文件头部诊断
      diagnoseDbHeader(extDbSelectedPath);
      showExtDbResult(false, `读取表列表失败: ${e}`);
    }
  }

  /** 诊断数据库文件头部 */
  async function diagnoseDbHeader(path: string) {
    try {
      const info = await dbApi.checkDbHeader(path) as {
        is_sqlite: boolean;
        size_bytes: number;
        page_count: number;
      };
      console.warn('[DB诊断]', extDbSelectedName, info);
    } catch {}
  }

  /** 加载表数据（根据模式自动选择） */
  /** 切换排序列/方向 */
  function toggleSort(colName: string) {
    if (dbSortCol === colName) {
      dbSortDir = dbSortDir === 'asc' ? 'desc' : 'asc';
    } else {
      dbSortCol = colName;
      dbSortDir = 'asc';
    }
    if (dbCurTable) loadDbTableData(dbCurTable, 0);
  }

  /** 点击列菜单外部时关闭列显隐菜单 */
  function handleDocClickDb(e: MouseEvent) {
    const t = e.target as HTMLElement | null;
    if (!t?.closest('.dbm-colmenu-wrap')) dbColSelectorOpen = false;
  }

  async function loadDbTableData(table: string, page = 0, dir?: 'next' | 'prev') {
    if (!table) return;
    // 切换表时重置排序（后端自动选择 rowid/首列作为默认排序列）
    if (table !== dbCurTable) {
      dbSortCol = '';
      dbSortDir = 'desc';
      dbDetailRow = null;
      dbDetailRowId = 0;
      dbSelectedRows = new Set();
      dbSchemaTab = 'indexes';
    }
    dbCurTable = table;
    dbTableError = '';
    dbLoadRetry = 0;
    try {
      const filter = dbFilterText.trim();
      // keyset 游标：prev/next 用当前页首/末行游标，首页/重查不用游标
      const cursor = dir === 'next' ? dbNextCursor : dir === 'prev' ? dbPrevCursor : null;
      const args = {
        table,
        page,
        pageSize: dbPageSize,
        orderCol: dbSortCol,
        orderDir: dbSortDir,
        filter,
        recount: page === 0,
        cursor,
        direction: dir ?? '',
      };
      if (extDbSelectedPath) {
    dbTableData = await dbApi.externalQueryTable(extDbSelectedPath, args);
      } else {
    dbTableData = await dbApi.queryTable(args);
      }
      // 数据加载成功后初始化列显隐（覆盖为新表的全部列）
      const loaded = dbTableData;
      if (loaded && loaded.columns.length > 0) {
        dbVisibleCols = new Set(loaded.columns.map((c) => c.name));
      }
      // 首页才重算总数；翻页时沿用缓存，避免大表每页全表 COUNT
      if (page === 0) dbTotal = loaded?.total ?? 0;
      dbPage = page;
      dbJumpPage = page + 1;
      dbNextCursor = loaded?.next_cursor ?? null;
      dbPrevCursor = loaded?.prev_cursor ?? null;
    } catch (e) {
      dbTableData = null;
      dbTableError = String(e);
      showExtDbResult(false, `查询数据失败: ${e}`);
      // 数据库可能正被微信解密进程重写（瞬时损坏/锁定），自动重试一次
      if (dbLoadRetry < 1 && dbCurTable === table) {
        dbLoadRetry += 1;
        setTimeout(() => {
          if (dbCurTable === table) loadDbTableData(table, page, dir);
        }, 900);
      }
    }
  }

  /** 当前数据源列宽 key 前缀 */
  function dbWidthKey() {
    return dbWidthKeyFromPath(extDbSelectedPath);
  }

  /** 完整列宽配置键（模板拖拽/步进/渲染共用） */
  function fullWidthKey(table: string, col: string) {
    return colWidthKey(dbWidthKey(), table, col);
  }

  /** 加载所有持久化列宽，格式 "col_width:<dbKey>:<table>:<col>" → 像素值 */
  async function loadDbColWidths() {
    dbWidthsLoaded = false;
    try {
    const items = await dbApi.getDbConfig();
      dbColWidths = parseColWidths(items);
    } catch {
      dbColWidths = {};
    } finally { dbWidthsLoaded = true; }
  }

  /** 保存单列宽度（按数据源隔离） */
  async function saveDbColWidth(table: string, col: string, width: number) {
    const key = colWidthKey(dbWidthKey(), table, col);
    try { await dbApi.setDbConfig(key, String(width)); } catch {}
  }

  /** 导出当前页为 CSV（弹窗选择保存位置，BOM + UTF-8，兼容 Excel） */
  async function exportCsv() {
    const data = dbTableData;
    if (!data || !dbCurTable) return;
    const cols = data.columns.filter((c: DbColumn) => dbVisibleCols?.has(c.name));
    if (!cols.length) return;
    const lines = [
      cols.map((c: DbColumn) => csvEscape(c.name)).join(','),
      ...data.rows.map((r: DbRow) => cols.map((c: DbColumn) => csvEscape(r[c.name])).join(',')),
    ];
    try {
      const { save } = await import('@tauri-apps/plugin-dialog');
      const path = await save({
        title: '导出 CSV',
        defaultPath: `${dbCurTable}_第${dbPage + 1}页.csv`,
        filters: [{ name: 'CSV 表格', extensions: ['csv'] }],
      });
      if (!path) return; // 用户取消，不做提示
      await dbApi.writeFile(
        path,
        utf8ToBase64('\uFEFF' + lines.join('\r\n')),
      );
      notify('数据库', `已导出 ${data.rows.length} 行 → ${path}`, 'success');
    } catch (e) {
      notify('数据库', `导出失败: ${e}`, 'error');
    }
  }

  /** 复制当前详情行（JSON）到剪贴板 */
  async function copyRowDetail() {
    if (!dbDetailRow) return;
    const ok = await copyText(JSON.stringify(dbDetailRow, null, 2));
    if (ok) notify('数据库', '已复制整行 JSON 到剪贴板', 'success');
    else notify('数据库', '复制失败', 'error');
  }

  /** 复制单个字段值 */
  async function copyField(key: string, val: unknown) {
    const ok = await copyText(val === null || val === undefined ? '' : String(val));
    if (ok) notify('数据库', `已复制字段 ${key}`, 'success');
    else notify('数据库', '复制失败', 'error');
  }

  /** 打开原始内容查看器（完整 BLOB / 文本） */
  async function openBlobViewer(column: string) {
    if (!dbCurTable) return;
    if (!dbDetailRowId) {
      showExtDbResult(false, '该表无 rowid，无法定位行以读取原始内容');
      return;
    }
    blobTab = 'preview';
    blobViewer = { column, rowid: dbDetailRowId, data: null };
    blobLoading = true;
    try {
      const data = await dbApi.getCellValue({
        table: dbCurTable,
        rowid: dbDetailRowId,
        column,
        dbPath: extDbSelectedPath,
      });
      blobViewer = { column, rowid: dbDetailRowId, data };
    } catch (e) {
      blobViewer = { column, rowid: dbDetailRowId, data: { kind: 'error', text: String(e) } };
    } finally {
      blobLoading = false;
    }
  }

  /** 下载当前 BLOB */
  /** 下载当前 BLOB（弹窗选择保存位置） */
  async function downloadBlob() {
    const v = blobViewer;
    if (!v || v.data?.kind !== 'blob') return;
    try {
      const { save } = await import('@tauri-apps/plugin-dialog');
      const ext = blobExt(v.data.mime || '');
      const path = await save({
        title: '保存文件',
        defaultPath: `${dbCurTable}_r${v.rowid}_${v.column}.${ext}`,
      });
      if (!path) return; // 用户取消
      await dbApi.writeFile(path, v.data.base64);
      notify('数据库', `已保存 → ${path}`, 'success');
    } catch (e) {
      notify('数据库', `保存失败: ${e}`, 'error');
    }
  }

  // ── CRUD 操作 ──

  /** 打开新增行表单（仅内置数据库） */
  function openInsertRow() {
    if (!dbCurTable || !isInternalDb) return;
    crudValues = Object.fromEntries(crudColumns.map((c: DbColumn) => [c.name, '']));
    crudNulls = Object.fromEntries(crudColumns.map((c: DbColumn) => [c.name, false]));
    crudError = '';
    crudMode = 'insert';
  }

  /** 打开编辑行表单（基于当前选中的详情行） */
  function openEditRow() {
    if (!dbCurTable || !isInternalDb || !dbDetailRow || !dbDetailRowId) return;
    const vals: Record<string, string> = {};
    const nulls: Record<string, boolean> = {};
    for (const c of crudColumns) {
      const raw = dbDetailRow[c.name];
      nulls[c.name] = raw === null || raw === undefined;
      vals[c.name] = raw === null || raw === undefined ? '' : String(raw);
    }
    crudValues = vals;
    crudNulls = nulls;
    crudError = '';
    crudMode = 'edit';
  }

  /** 提交新增/编辑 */
  async function saveCrudRow() {
    if (!dbCurTable || !crudMode || crudSaving) return;
    const data: Record<string, unknown> = {};
    for (const c of crudColumns) {
      if (crudNulls[c.name]) { data[c.name] = null; continue; }
      data[c.name] = (crudValues[c.name] ?? '').trim();
    }
    crudSaving = true;
    crudError = '';
    try {
      if (crudMode === 'insert') {
    await dbApi.insertRow(dbCurTable, data);
      } else {
        if (!dbDetailRowId) throw new Error('缺少 rowid，无法定位行');
    await dbApi.updateRow(dbCurTable, dbDetailRowId, data);
      }
      const mode = crudMode;
      crudMode = null;
      await loadDbTableData(dbCurTable, 0);
      notify('数据库', mode === 'insert' ? '已插入新行' : '已更新该行', 'success');
    } catch (e) {
      crudError = String(e);
    } finally {
      crudSaving = false;
    }
  }

  /** 请求删除当前行（弹确认框） */
  function requestDeleteRow() {
    if (!dbCurTable || !isInternalDb || !dbDetailRowId) return;
    deleteTarget = { rowid: dbDetailRowId };
  }

  /** 执行删除 */
  async function confirmDeleteRow() {
    if (!dbCurTable || !deleteTarget || deleteSaving) return;
    deleteSaving = true;
    try {
    await dbApi.deleteRow(dbCurTable, deleteTarget.rowid);
      deleteTarget = null;
      dbDetailRow = null;
      dbDetailRowId = 0;
      await loadDbTableData(dbCurTable, 0);
      notify('数据库', '已删除该行', 'success');
    } catch (e) {
      notify('数据库', `删除失败: ${e}`, 'error');
      deleteTarget = null;
    } finally {
      deleteSaving = false;
    }
  }

  /** 计算每列自适应宽度（仅测量表头 + 前 50 行数据，最大化性能） */
  function computeAutoWidths() {
    const data = dbTableData;
    if (!data || !dbCurTable) return;
    const font = '600 12px ' + getComputedStyle(document.body).fontFamily;
    const sampleRows = data.rows.slice(0, 50);
    const widths: Record<string, number> = {};
    for (const col of data.columns) {
      let max = measureTextWidth(col.name, font) + 20; // 表头宽度 + 缓冲
      for (const row of sampleRows) {
        const v = String(row[col.name] ?? '');
        const w = measureTextWidth(v, font) + 16;
        if (w > max) max = w;
      }
      widths[col.name] = Math.max(50, Math.min(600, Math.ceil(max)));
    }
    dbAutoWidths = widths;
  }

  /** 数据加载后异步计算列宽（不影响首屏渲染） */
  $effect(() => {
    if (dbTableData && dbWidthsLoaded) {
      // 用 queueMicrotask 推到 DOM 更新后再测量
      queueMicrotask(() => requestAnimationFrame(computeAutoWidths));
    }
  });

  // 启动时预加载（保证切到该面板时数据源/表列表已就绪）
  onMount(() => {
    initDbScanDirs().then(() => refreshExtDbFiles());
    document.addEventListener('click', handleDocClickDb);
    document.addEventListener('keydown', onShortcutKeydown);
    setupDragDrop();
    loadPins();
    (async () => {
      try {
    const items = await dbApi.getDbConfig();
        autoRefresh = items.find(i => i.key === 'db_auto_refresh')?.value === '1';
      } catch {}
    })();
    return () => {
      document.removeEventListener('click', handleDocClickDb);
      document.removeEventListener('keydown', onShortcutKeydown);
      if (dragUnlisten) dragUnlisten();
      if (autoRefreshTimer) clearInterval(autoRefreshTimer);
    };
  });


  // 进入数据库管理面板时刷新数据
  $effect(() => {
    if (active) {
      loadDbTables();
      initDbScanDirs().then(() => refreshExtDbFiles());
      loadDbColWidths();
    }
  });

  // ── 表列表搜索 ──
  let dbTableSearch = $state('');

  /** 表列表分组：收藏 → 全部 */
  const dbTableSections = $derived.by(() => groupDbTables(dbTables, dbPins, dbTableSearch));

  /** 外部数据库按「扫描根目录」分组（未命中扫描根的按所在目录分组） */
  const groupedDbFiles = $derived(groupDbFilesByRoot(extDbFiles, dbScanDirs));

  /** 千分位格式化 */
  function fmtNum(n: number): string {
    return (n ?? 0).toLocaleString('en-US');
  }

  /** 一键刷新：数据源 + 表列表 + 当前表数据（带反馈） */
  async function refreshAll() {
    await Promise.allSettled([loadDbTables(), refreshExtDbFiles()]);
    if (dbCurTable) await loadDbTableData(dbCurTable, 0);
    notify('数据库', '已刷新数据源与当前表', 'success');
  }

  /** 刷新当前表数据（带反馈） */
  async function refreshCurrentTable() {
    if (!dbCurTable) return;
    await loadDbTableData(dbCurTable, 0);
    notify('数据库', `已刷新表 ${dbCurTable}`, 'success');
  }

  /** 重新扫描数据库目录（带反馈） */
  async function rescanDirs() {
    await refreshExtDbFiles();
    notify('数据库', `扫描完成：共 ${extDbFiles.length} 个数据库文件`, 'success');
  }

  // ── 分页跳转 ──
  let dbJumpPage = $state(1);

  /** 跳到指定页（0 起；超出范围自动收敛） */
  function goToPage(p: number) {
    if (!dbCurTable) return;
    const total = Math.max(1, Math.ceil(dbTotal / dbPageSize));
    const page = Math.max(0, Math.min(total - 1, Math.floor(p)));
    dbJumpPage = page + 1;
    loadDbTableData(dbCurTable, page);
  }

  /** 当前表总页数 */
  const dbTotalPages = $derived(Math.max(1, Math.ceil(dbTotal / dbPageSize)));

  // ── 界面状态：侧栏折叠 / 顶栏数据源切换 ──
  let sidebarCollapsed = $state(false);

  function toggleSidebar() {
    sidebarCollapsed = !sidebarCollapsed;
  }

  function onSourceSelectValue(v: string) {
    if (v === 'internal') {
      selectInternalDb();
    } else if (v) {
      const f = extDbFiles.find(x => x.path === v);
      if (f) selectExternalDb(f.path, f.name);
    }
  }
  const curDbLabel = $derived(
    extDbSelectedPath
      ? (extDbFiles.find((x) => x.path === extDbSelectedPath)?.name ?? extDbSelectedPath)
      : '内置 control.db'
  );

  // ═══════════════ 增强功能（表详情 / SQL / 统计 / 对比 / 备份 / 收藏 / 多选等） ═══════════════

  // ── 表详情 / DDL / 表操作菜单 ──
  let tableDetail = $state<DbTableDetail | null>(null);
  let tableDetailLoading = $state(false);
  let ddlModalOpen = $state(false);
  let dbSchemaTab = $state<'indexes' | 'triggers' | 'fk' | 'ddl'>('indexes');

  async function loadTableDetail(table?: string) {
    const t = table ?? dbCurTable;
    if (!t) { tableDetail = null; return; }
    tableDetailLoading = true;
    try {
    tableDetail = await dbApi.getTableDetail(extDbSelectedPath, t);
    } catch { tableDetail = null; }
    finally { tableDetailLoading = false; }
  }

  async function openDdlModal(table?: string) {
    const t = table ?? dbCurTable;
    if (!t) return;
    ddlModalOpen = true;
    await loadTableDetail(t);
  }

  async function copyTableName(table: string) {
    const ok = await copyText(table);
    if (ok) notify('数据库', `已复制表名: ${table}`, 'success');
    else notify('数据库', '复制失败', 'error');
  }

  /** 复制建表语句 */
  async function copyDdl() {
    const ddl = tableDetail?.ddl;
    if (!ddl) return;
    const ok = await copyText(ddl);
    if (ok) notify('数据库', '已复制建表语句', 'success');
    else notify('数据库', '复制失败', 'error');
  }

  /** 导出整表 CSV（后端分块流式） */
  async function exportWholeTable(table: string) {
    try {
      const { save } = await import('@tauri-apps/plugin-dialog');
      const path = await save({
        title: '导出整表为 CSV',
        defaultPath: `${table}.csv`,
        filters: [{ name: 'CSV 表格', extensions: ['csv'] }],
      });
      if (!path) return;
    const r = await dbApi.exportTableCsv(extDbSelectedPath, table, path);
      notify('数据库', `已导出 ${r.count} 行 → ${path}`, 'success');
    } catch (e) { notify('数据库', `导出失败: ${e}`, 'error'); }
  }

  // ── SQL 控制台 ──
  let sqlText = $state('');
  let sqlResult = $state<DbSqlResult | null>(null);
  let sqlLoading = $state(false);
  let sqlError = $state('');

  async function runSql() {
    if (!sqlText.trim() || sqlLoading) return;
    sqlLoading = true; sqlError = ''; sqlResult = null;
    try {
    sqlResult = await dbApi.runSql(extDbSelectedPath, sqlText, 500);
    } catch (e) { sqlError = String(e); }
    finally { sqlLoading = false; }
  }

  function insertSqlSample() {
    sqlText = `SELECT * FROM ${dbCurTable || '表名'} LIMIT 50`;
  }

  // ── 列统计 ──
  let statsOpen = $state(false);
  let statsLoading = $state(false);
  let statsData = $state<DbTableStats | { error: string } | null>(null);

  async function openStats() {
    if (!dbCurTable) return;
    statsOpen = true; statsLoading = true; statsData = null;
    try {
    statsData = await dbApi.tableStats(extDbSelectedPath, dbCurTable, 2000);
    } catch (e) { statsData = { error: String(e) }; }
    finally { statsLoading = false; }
  }

  // ── 完整性检查 ──
  let integrityBusy = $state(false);
  let integrityResult = $state<DbIntegrityResult | { error: string } | null>(null);

  async function runIntegrity() {
    if (integrityBusy) return;
    integrityBusy = true; integrityResult = null;
    try {
    integrityResult = await dbApi.dbIntegrity(extDbSelectedPath);
    } catch (e) { integrityResult = { error: String(e) }; }
    finally { integrityBusy = false; }
  }

  // ── 内置库：指标 / 事件 / 清理 / 备份恢复 ──
  let internalDbInfo = $state<DbInfo | null>(null);
  let dbEvents = $state<DbEvent[]>([]);
  let cleanupBusy = $state(false);
  let cleanupResult = $state<DbCleanupResult | null>(null);
  let retentionDays = $state(90);
  let backupBusy = $state(false);
  let restoreHint = $state<string | null>(null);

  async function refreshInternalInfo() {
    try {
    internalDbInfo = await dbApi.getDbInfo();
    const items = await dbApi.getDbConfig();
      retentionDays = parseInt(items.find(i => i.key === 'retention_days')?.value ?? '90') || 90;
    } catch {}
  }

  async function loadEvents(limit = 50) {
    try { dbEvents = await dbApi.queryEvents(limit, 0); }
    catch { dbEvents = []; }
  }

  async function triggerCleanup() {
    if (cleanupBusy) return;
    cleanupBusy = true;
    try {
    const r = await dbApi.cleanupOldData();
      cleanupResult = r;
      notify('数据库', `已清理 ${r.deleted_events} 条事件、${r.deleted_agent} 条日志`, 'success');
    } catch (e) { notify('数据库', `清理失败: ${e}`, 'error'); }
    finally { cleanupBusy = false; }
  }

  async function backupDb() {
    if (backupBusy) return;
    try {
      const { save } = await import('@tauri-apps/plugin-dialog');
      const path = await save({
        title: '备份 control.db',
        defaultPath: `control_${new Date().toISOString().slice(0, 10)}.db`,
        filters: [{ name: 'SQLite 数据库', extensions: ['db'] }],
      });
      if (!path) return;
      backupBusy = true;
    const r = await dbApi.backupInternalDb(path);
      notify('数据库', `备份完成 → ${r.path}`, 'success');
    } catch (e) { notify('数据库', `备份失败: ${e}`, 'error'); }
    finally { backupBusy = false; }
  }

  async function restoreDb() {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const sel = await open({
        multiple: false,
        title: '选择 control.db 备份文件',
        filters: [{ name: '数据库文件', extensions: ['db'] }],
      });
      if (typeof sel !== 'string' || !sel.trim()) return;
    const r = await dbApi.restoreInternalDb(sel);
      restoreHint = r.hint;
      notify('数据库', '已生成恢复文件，请按提示操作', 'warn');
    } catch (e) { notify('数据库', `恢复失败: ${e}`, 'error'); }
  }

  // ── 外部库文件诊断 ──
  let dbDiag = $state<DbHeaderInfo | { error: string } | null>(null);
  let diagBusy = $state(false);

  async function diagnoseDb() {
    if (!extDbSelectedPath || diagBusy) return;
    diagBusy = true;
    try { dbDiag = await dbApi.checkDbHeader(extDbSelectedPath); }
    catch (e) { dbDiag = { error: String(e) }; }
    finally { diagBusy = false; }
  }

  // ── 收藏（持久化到 _config） ──
  let dbPins = $state<string[]>([]);

  function pinsKey() { return `db_pins:${dbWidthKey()}`; }

  async function loadPins() {
    try {
    const items = await dbApi.getDbConfig();
      for (const it of items) {
        if (it.key === pinsKey()) {
          try { const a = JSON.parse(it.value); if (Array.isArray(a)) dbPins = a.filter((x): x is string => typeof x === 'string'); } catch {}
        }
      }
    } catch {}
  }

  async function savePins() {
    try {
    await dbApi.setDbConfig(pinsKey(), JSON.stringify(dbPins));
    } catch {}
  }

  function togglePin(table: string) {
    dbPins = dbPins.includes(table) ? dbPins.filter(t => t !== table) : [...dbPins, table];
    savePins();
  }

  // ── 自动刷新 ──
  let autoRefresh = $state(false);
  let autoRefreshTimer: ReturnType<typeof setInterval> | null = null;

  async function toggleAutoRefresh() {
    autoRefresh = !autoRefresh;
    try { await dbApi.setDbConfig('db_auto_refresh', autoRefresh ? '1' : '0'); } catch {}
  }

  function setupAutoRefresh() {
    if (autoRefreshTimer) { clearInterval(autoRefreshTimer); autoRefreshTimer = null; }
    if (autoRefresh) {
      autoRefreshTimer = setInterval(() => {
        if (active && dbCurTable) loadDbTableData(dbCurTable, dbPage);
      }, 5000);
    }
  }

  $effect(() => { void autoRefresh; setupAutoRefresh(); });

  // ── 多选行 ──
  let selectMode = $state(false);
  let dbSelectedRows = $state<Set<number>>(new Set());

  function toggleSelectMode() {
    selectMode = !selectMode;
    if (!selectMode) dbSelectedRows = new Set();
  }

  function toggleRowSelect(rid: number) {
    const s = new Set(dbSelectedRows);
    if (s.has(rid)) s.delete(rid); else s.add(rid);
    dbSelectedRows = s;
  }

  function toggleSelectAll() {
    const ids = (dbTableData?.rows ?? [])
      .map((r: DbRow) => parseInt(String(r.rowid ?? '0')))
      .filter((n: number) => n > 0);
    const allSelected = ids.length > 0 && ids.every((n: number) => dbSelectedRows.has(n));
    const s = new Set(dbSelectedRows);
    if (allSelected) { for (const n of ids) s.delete(n); } else { for (const n of ids) s.add(n); }
    dbSelectedRows = s;
  }

  function clearRowSelection() { dbSelectedRows = new Set(); }

  /** 导出选中行 CSV（基于当前可见列） */
  async function exportSelectedCsv() {
    const data = dbTableData;
    if (!data || !dbCurTable || dbSelectedRows.size === 0) return;
    const selectedRows = data.rows.filter((r: DbRow) => dbSelectedRows.has(parseInt(String(r.rowid ?? '0'))));
    const cols = data.columns.filter((c: DbColumn) => dbVisibleCols?.has(c.name));
    if (!cols.length || !selectedRows.length) return;
    const lines = [
      cols.map((c: DbColumn) => csvEscape(c.name)).join(','),
      ...selectedRows.map((r: DbRow) => cols.map((c: DbColumn) => csvEscape(r[c.name])).join(',')),
    ];
    try {
      const { save } = await import('@tauri-apps/plugin-dialog');
      const path = await save({
        title: '导出选中行',
        defaultPath: `${dbCurTable}_选中${selectedRows.length}行.csv`,
        filters: [{ name: 'CSV 表格', extensions: ['csv'] }],
      });
      if (!path) return;
    await dbApi.writeFile(path, utf8ToBase64('\uFEFF' + lines.join('\r\n')));
      notify('数据库', `已导出 ${selectedRows.length} 行 → ${path}`, 'success');
    } catch (e) { notify('数据库', `导出失败: ${e}`, 'error'); }
  }

  // ── 表结构对比 ──
  let compareOpen = $state(false);
  let compareSrcA = $state('internal');
  let compareSrcB = $state('internal');
  let compareTablesA = $state<string[]>([]);
  let compareTablesB = $state<string[]>([]);
  let compareTableA = $state('');
  let compareTableB = $state('');
  let compareResult = $state<
    | {
        onlyA: DbColumn[];
        onlyB: DbColumn[];
        changed: { name: string; a?: string; b?: string }[];
      }
    | { error: string }
    | null
  >(null);
  let compareLoading = $state(false);

  function compareSrcOptions() {
    const list = [{ label: '内置 control.db', value: 'internal' }];
    for (const f of extDbFiles) list.push({ label: f.name, value: f.path });
    return list;
  }

  async function onCompareSrcChanged(side: 'A' | 'B') {
    const src = side === 'A' ? compareSrcA : compareSrcB;
    try {
      const t = src === 'internal'
    ? await dbApi.listTables()
    : await dbApi.externalListTables(src);
      if (side === 'A') { compareTablesA = t; compareTableA = t[0] ?? ''; }
      else { compareTablesB = t; compareTableB = t[0] ?? ''; }
    } catch { /* ignore */ }
  }

  function openCompare() {
    compareOpen = true; compareResult = null;
    compareSrcA = 'internal'; compareSrcB = 'internal';
    onCompareSrcChanged('A'); onCompareSrcChanged('B');
  }

  async function doCompare() {
    if (!compareTableA || !compareTableB) { notify('数据库', '请选择两张要对比的表', 'warn'); return; }
    compareLoading = true; compareResult = null;
    try {
      const fetchSchema = (src: string, table: string) => src === 'internal'
    ? dbApi.tableSchema(table)
    : dbApi.externalTableSchema(src, table);
      const [colsA, colsB] = await Promise.all([
        fetchSchema(compareSrcA, compareTableA),
        fetchSchema(compareSrcB, compareTableB),
      ]);
      const mB = new Map(colsB.map((c) => [c.name, c]));
      const mA = new Map(colsA.map((c) => [c.name, c]));
      const onlyA = colsA.filter((c) => !mB.has(c.name));
      const onlyB = colsB.filter((c) => !mA.has(c.name));
      const changed = colsA
        .filter((c) => mB.has(c.name) && mB.get(c.name)?.col_type !== c.col_type)
        .map((c) => ({ name: c.name, a: c.col_type, b: mB.get(c.name)?.col_type }));
      compareResult = { onlyA, onlyB, changed };
    } catch (e) { compareResult = { error: String(e) }; }
    finally { compareLoading = false; }
  }

  // ── 拖拽打开 / 快捷键 / 微信时间戳 ──
  let dragUnlisten: (() => void) | null = null;
  let filterInputEl = $state<HTMLInputElement | null>(null);

  async function setupDragDrop() {
    try {
      const { getCurrentWebview } = await import('@tauri-apps/api/webview');
      dragUnlisten = await getCurrentWebview().onDragDropEvent(async (e) => {
        if (e.payload?.type !== 'drop' || !active) return;
        const paths: string[] = e.payload.paths ?? [];
        const dbFile = paths.find((p: string) => /\.(db|sqlite|sqlite3|db3|sdb)$/i.test(p));
        if (!dbFile) return;
        const idx = Math.max(dbFile.lastIndexOf('\\'), dbFile.lastIndexOf('/'));
        const dir = idx > 0 ? dbFile.slice(0, idx) : '';
        if (dir && !dbScanDirs.includes(dir)) {
          dbScanDirs = [...dbScanDirs, dir];
          await saveDbScanDirs();
        }
        await refreshExtDbFiles();
        const name = dbFile.split(/[\\/]/).pop() || dbFile;
        await selectExternalDb(dbFile, name);
        notify('数据库', `已通过拖拽打开: ${name}`, 'success');
      });
    } catch (e) { console.warn('拖拽打开不可用:', e); }
  }

  function onShortcutKeydown(e: KeyboardEvent) {
    if (!active) return;
    const target = e.target as HTMLElement;
    if (target && ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName)) {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'f') {
        e.preventDefault();
        filterInputEl?.focus();
      }
      return;
    }
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'e') { e.preventDefault(); exportCsv(); return; }
    if (e.key === 'F2' && isInternalDb && dbDetailRowId) { e.preventDefault(); openEditRow(); return; }
    if (e.key === 'Delete' && isInternalDb && dbDetailRowId) { e.preventDefault(); requestDeleteRow(); }
  }

</script>
        <!-- 数据库管理 · 全新三栏布局 -->
        <div class="dbm">
          <!-- 顶部标题栏：数据源下拉 + 全局操作 -->
          <header class="dbm-header">
            <div class="dbm-title">
              <span class="dbm-title-ico"><DatabaseIcon class="size-4.5" /></span>
              <h2>数据库管理</h2>
              <div class="dbm-src-select-wrap" title="切换数据源">
                <SelectRoot type="single" value={extDbSelectedPath ?? 'internal'} onValueChange={onSourceSelectValue}>
                  <SelectTrigger class="dbm-src-select h-8 w-72 justify-between">
                    <span class="truncate">{curDbLabel}</span>
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="internal">🔒 内置 control.db</SelectItem>
                    {#each groupedDbFiles as g}
                      <SelectGroup>
                        <SelectLabel>{g.dirName}</SelectLabel>
                        {#each g.files as f}
                          <SelectItem value={f.path}>{f.name}（{fmtBytes(f.size_bytes)}）</SelectItem>
                        {/each}
                      </SelectGroup>
                    {/each}
                  </SelectContent>
                </SelectRoot>
              </div>
              {#if dbTables.length > 0}
                <Badge variant="secondary" class="h-6 px-2.5 text-xs" title="当前数据源表数量">{dbTables.length} 张表</Badge>
              {/if}
            </div>
            <div class="dbm-actions">
              <Button size="sm" variant="outline" onclick={addDbScanDir} title="将目录加入扫描列表（持久化到配置）">
                <FolderPlusIcon class="size-3.5" />目录
              </Button>
              <Button size="sm" variant="outline" onclick={openDbFile} title="从任意位置打开一个数据库文件">
                <FilePlusIcon class="size-3.5" />文件
              </Button>
              <Button size="sm" variant="outline" onclick={rescanDirs} title="重新扫描数据库目录">
                <RefreshCwIcon class="size-3.5 {extDbLoading ? 'animate-spin' : ''}" />扫描
              </Button>
              {#if !extDbSelectedPath}
                <Button size="sm" variant="outline" onclick={backupDb} disabled={backupBusy} title="备份内置 control.db">
                  <SaveIcon class="size-3.5" />备份
                </Button>
              {/if}
              <Button size="sm" variant="outline" onclick={openCompare} title="表结构对比">
                <GitCompareArrowsIcon class="size-3.5" />对比
              </Button>
              <RippleButton
                onclick={refreshAll}
                title="刷新数据源、表列表与当前数据"
                rippleColor="#a5f3fc"
                class="h-8 rounded-md border-0 bg-[var(--primary)] px-3.5 text-xs font-medium text-[var(--primary-foreground)] hover:opacity-90"
              >
                <RefreshCwIcon class="size-3.5" />刷新
              </RippleButton>
            </div>
          </header>

          {#if appDbs.length > 0}
            <div class="dbm-appdbs">
              <span class="dbm-appdbs-label">常用数据库</span>
              {#each appDbs as d (d.key)}
                <button
                  class="dbm-appdb"
                  class:dbm-appdb-on={extDbSelectedPath === d.path}
                  onclick={() => selectExternalDb(d.path, d.name)}
                  title={d.path}
                >{d.label}</button>
              {/each}
              <button
                class="dbm-appdb"
                class:dbm-appdb-on={!extDbSelectedPath}
                onclick={selectInternalDb}
                title="应用内置数据库（事件/任务/聊天记录）"
              ><LockIcon class="size-3.5" /> 内置 control.db</button>
            </div>
          {/if}

          <div class="dbm-body">
            <!-- 左侧：表列表（数据源已移至顶栏下拉） -->
            <aside class="dbm-side" class:dbm-side-collapsed={sidebarCollapsed}>
              <div class="dbm-side-sec dbm-side-sec-tables">
                <div class="dbm-side-hd">
                  <span style="display:inline-flex;align-items:center;gap:6px"><Table2Icon class="size-3.5" /> 表列表</span>
                  <span style="display:flex;align-items:center;gap:6px">
                    {#if !sidebarCollapsed}<span class="dbm-badge">{dbTables.length}</span>{/if}
                    <button class="dbm-mini" onclick={toggleSidebar} title="折叠/展开侧栏">{sidebarCollapsed ? '»' : '«'}</button>
                  </span>
                </div>
                <div class="dbm-table-search">
                  <div class="dbm-table-search-box">
                    <SearchIcon style="position:absolute;left:8px;width:13px;height:13px;pointer-events:none;opacity:.65;color:var(--dg-muted)" />
                    <Input class="h-8 pl-[26px] text-xs" placeholder="搜索表名..." bind:value={dbTableSearch} />
                    {#if dbTableSearch}<button class="dbm-filter-clear" onclick={() => dbTableSearch = ''} title="清除搜索"><XIcon class="size-3" /></button>{/if}
                  </div>
                </div>
                <div class="dbm-table-list">
                  {#each dbTableSections as sec}
                    {#if sec.tables.length > 0}
                      <div class="dbm-table-sec-hd">
                        <span>{sec.label}</span>
                        <span class="dbm-badge">{sec.tables.length}</span>
                      </div>
                    {/if}
                    {#each sec.tables as t}
                      <div class="dbm-table" class:dbm-table-active={dbCurTable === t} role="button" tabindex="0"
                        onclick={() => { switchDbTab('browse'); loadDbTableData(t, 0); }}
                        onkeydown={(e) => e.key === 'Enter' && (switchDbTab('browse'), loadDbTableData(t, 0))}
                        title={t}>
                        <span class="dbm-table-ico">
                          {#if t.startsWith('sqlite_')}
                            <CogIcon class="size-3.5" />
                          {:else if t.startsWith('_')}
                            <LockIcon class="size-3.5" />
                          {:else}
                            <Table2Icon class="size-3.5" />
                          {/if}
                        </span>
                        <span class="dbm-table-name">{t}</span>
                        <span class="dbm-table-actions" role="presentation" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
                          <button class="dbm-table-act" onclick={() => togglePin(t)} title={dbPins.includes(t) ? '取消收藏' : '收藏该表'}>
                            {dbPins.includes(t) ? '★' : '☆'}
                          </button>
                          <DropdownMenu>
                            <DropdownMenuTrigger>
                              <button class="dbm-table-act" title="更多操作">⋯</button>
                            </DropdownMenuTrigger>
                            <DropdownMenuContent align="end" class="min-w-44">
                              <DropdownMenuItem onclick={() => openDdlModal(t)}><FileTextIcon class="size-3.5" /> 建表 DDL</DropdownMenuItem>
                              <DropdownMenuItem onclick={() => copyTableName(t)}><ClipboardCopyIcon class="size-3.5" /> 复制表名</DropdownMenuItem>
                              <DropdownMenuItem onclick={() => exportWholeTable(t)}><DownloadIcon class="size-3.5" /> 导出整表 CSV</DropdownMenuItem>
                              <DropdownMenuItem onclick={() => togglePin(t)}><StarIcon class="size-3.5" /> {dbPins.includes(t) ? '取消收藏' : '收藏'}</DropdownMenuItem>
                            </DropdownMenuContent>
                          </DropdownMenu>
                        </span>
                      </div>
                    {/each}
                  {/each}
                  {#if dbTableSections.every(s => s.tables.length === 0)}
                    <div class="dbm-empty-mini">
                      {#if dbTables.length === 0}
                        该数据源暂无表
                      {:else}
                        没有匹配「{dbTableSearch}」的表
                        <button class="dbm-link-btn" onclick={() => dbTableSearch = ''}>清除搜索</button>
                      {/if}
                    </div>
                  {/if}
                </div>
              </div>
            </aside>
            <!-- 右侧主区域 -->
            <main class="dbm-main">
              <Tabs value={dbSubTab} onValueChange={(v) => switchDbTab(v as 'browse'|'schema'|'status'|'sql')} class="dbm-tabs">
                <TabsList class="h-9 w-fit">
                  <TabsTrigger value="browse">数据浏览</TabsTrigger>
                  <TabsTrigger value="schema">表结构</TabsTrigger>
                  <TabsTrigger value="status" title="内置库指标 / 文件诊断 / 完整性 / 备份 / 清理 / 事件">运维</TabsTrigger>
                  <TabsTrigger value="sql" title="SQL 控制台（外部库只读）">SQL</TabsTrigger>
                </TabsList>
              </Tabs>

              <div class="dbm-content">
                {#if dbSubTab === 'browse'}
                  <div class="dbm-toolbar">
                    <div class="dbm-crumb" title={`${extDbSelectedPath ?? '(内部数据库)'}`}>{extDbSelectedName}{#if dbCurTable} › <b>{dbCurTable}</b>{/if}</div>
                    <div class="dbm-toolbar-right">
                      <div class="dbm-filter">
                        <SearchIcon style="position:absolute;left:9px;width:14px;height:14px;pointer-events:none;opacity:.7;color:var(--dg-muted)" />
                        <Input class="h-8 w-56 pl-[30px] text-xs" placeholder="全表搜索（后端过滤）..." bind:value={dbFilterText} bind:ref={filterInputEl} />
                        {#if dbFilterText}
                          <button class="dbm-filter-clear" onclick={() => dbFilterText = ''} title="清除筛选"><XIcon class="size-3.5" /></button>
                        {/if}
                      </div>
                      <span class="dbm-tb-sep"></span>
                      <Button size="icon" variant="outline" class="h-8 w-8" onclick={openStats} disabled={!dbCurTable || !dbTableData} title="列统计（抽样 2000 行）">
                        <BarChart3Icon class="size-3.5" />
                      </Button>
                      <Button size="icon" variant={selectMode ? 'default' : 'outline'} class="h-8 w-8" onclick={toggleSelectMode} title="多选行（支持批量导出选中行）">
                        <CheckSquareIcon class="size-3.5" />
                      </Button>
                      <Button size="icon" variant={autoRefresh ? 'default' : 'outline'} class="h-8 w-8" onclick={toggleAutoRefresh} title={autoRefresh ? '自动刷新已开启（每 5 秒）' : '开启自动刷新'}>
                        <RefreshCwIcon class="size-3.5" />
                      </Button>
                      <span class="dbm-tb-sep"></span>
                      <Button size="sm" variant="outline" onclick={openInsertRow} disabled={!isInternalDb || !dbCurTable || !dbTableData}
                        title={isInternalDb ? '新增一行（仅内置数据库可写）' : '外部数据库为只读，仅内置 control.db 支持写入'}>
                        <PlusIcon class="size-3.5" />新增行
                      </Button>
                      <Button size="sm" variant="outline" onclick={exportCsv} disabled={!dbCurTable || !dbTableData} title="导出当前页为 CSV">
                        <DownloadIcon class="size-3.5" />CSV
                      </Button>
                      {#if selectMode}
                        <span class="dbm-selected-info">已选 <b>{dbSelectedRows.size}</b> 行</span>
                        <Button size="icon" variant="outline" class="h-8 w-8" onclick={exportSelectedCsv} disabled={dbSelectedRows.size === 0} title="导出选中行为 CSV">
                          <DownloadIcon class="size-3.5" />
                        </Button>
                        <Button size="icon" variant="outline" class="h-8 w-8" onclick={clearRowSelection} disabled={dbSelectedRows.size === 0} title="清除选择">
                          <XIcon class="size-3.5" />
                        </Button>
                      {/if}
                      <div class="dbm-colmenu-wrap">
                        <Button size="icon" variant="outline" class="h-8 w-8" onclick={() => dbColSelectorOpen = !dbColSelectorOpen} title="列显隐">
                          <Columns3Icon class="size-3.5" />
                        </Button>
                        {#if dbColSelectorOpen}
                          <div class="dbm-colmenu" role="dialog" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.key === 'Escape' && (dbColSelectorOpen = false)}>
                            <div class="dbm-colmenu-hd">
                              <span>显示字段</span>
                              <div style="display:flex;gap:4px">
                                <Button size="sm" variant="ghost" class="h-6 px-2 text-xs" onclick={() => { dbVisibleCols = new Set(dbTableData!.columns.map((c) => c.name)); }}>全选</Button>
                                <Button size="sm" variant="ghost" class="h-6 px-2 text-xs" onclick={() => { dbVisibleCols = new Set<string>(); }}>清空</Button>
                              </div>
                            </div>
                            <div class="dbm-colmenu-body">
                              {#each dbTableData?.columns ?? [] as col}
                                <label class="dbm-coli">
                                  <Checkbox checked={dbVisibleCols?.has(col.name)}
                                    onCheckedChange={() => { if (!dbVisibleCols) dbVisibleCols = new Set(); if (dbVisibleCols.has(col.name)) dbVisibleCols.delete(col.name); else dbVisibleCols.add(col.name); dbVisibleCols = dbVisibleCols; }} />
                                  <span>{col.name}</span>
                                  <span class="dbm-coli-ty">{col.col_type}</span>
                                </label>
                              {/each}
                            </div>
                          </div>
                        {/if}
                      </div>
                      <Button size="icon" variant="outline" class="h-8 w-8" onclick={refreshCurrentTable} title="刷新当前表数据">
                        <RefreshCwIcon class="size-3.5" />
                      </Button>
                    </div>
                  </div>

                  <div class="dbm-grid-wrap" role="application" aria-label="数据表格" tabindex="-1">
                    {#if dbCurTable && dbTableData}
                      <table class="dbm-grid" role="grid" tabindex="0"
                        onmousemove={(e) => {
                          if (dbResizing) {
                            const dx = e.clientX - dbResizing.startX;
                            const w = Math.max(40, dbResizing.startW + dx);
                            const rk = fullWidthKey(dbCurTable, dbResizing.col);
                            dbColWidths = { ...dbColWidths, [rk]: w };
                          }
                        }}
                        onmouseup={() => {
                          if (dbResizing) {
                            const rk = fullWidthKey(dbCurTable, dbResizing.col);
                            const w = dbColWidths[rk] ?? 120;
                            saveDbColWidth(dbCurTable, dbResizing.col, w);
                            dbResizing = null;
                          }
                        }}
                        onmouseleave={() => {
                          if (dbResizing) {
                            const rk = fullWidthKey(dbCurTable, dbResizing.col);
                            const w = dbColWidths[rk] ?? 120;
                            saveDbColWidth(dbCurTable, dbResizing.col, w);
                            dbResizing = null;
                          }
                        }}
                      >
                        <thead><tr>
                          <th class="dbm-th dbm-rid-th">#</th>
                          {#if selectMode}
                            <th class="dbm-th dbm-sel-th">
                              <Checkbox title="全选当前页"
                                onCheckedChange={toggleSelectAll}
                                checked={(() => { const ids = (dbTableData?.rows ?? []).map((r: DbRow) => parseInt(String(r.rowid ?? '0'))).filter((n: number) => n > 0); return ids.length > 0 && ids.every((n: number) => dbSelectedRows.has(n)); })()} />
                            </th>
                          {/if}
                          {#each dbTableData.columns as col (col.name)}
                            {#if dbVisibleCols?.has(col.name)}
                              <th class="dbm-th dbm-th-sort" style={(() => {
                                  const userW = dbWidthsLoaded ? dbColWidths[fullWidthKey(dbCurTable, col.name)] : undefined;
                                  const autoW = dbAutoWidths[col.name];
                                  const w = userW ?? autoW ?? 120;
                                  return `width:${w}px`;
                                })()}
                                onclick={() => toggleSort(col.name)}
                                onmouseenter={(e) => {
                                  const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
                                  dbColTip = { name: col.name, type: col.col_type ?? '', x: r.left + r.width / 2, y: r.top };
                                }}
                                onmouseleave={() => { dbColTip = null; }}>
                                <span class="dbm-th-name">{col.name}</span>
                                {#if dbSortCol === col.name}<span class="dbm-arr">{dbSortDir === 'asc' ? '▲' : '▼'}</span>{/if}
                                <span class="dbm-resize" role="slider" aria-label="调整列宽" tabindex="0"
                                  aria-valuemin={40} aria-valuemax={2000}
                                  aria-valuenow={dbColWidths[fullWidthKey(dbCurTable, col.name)] ?? 120}
                                  onmousedown={(e) => {
                                    e.stopPropagation();
                                    const key = fullWidthKey(dbCurTable, col.name);
                                    const curW = dbColWidths[key] ?? (e.currentTarget.parentElement as HTMLElement).offsetWidth;
                                    dbResizing = { col: col.name, startX: e.clientX, startW: curW };
                                  }}
                                  onkeydown={(e) => {
                                    if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
                                    e.preventDefault();
                                    const key = fullWidthKey(dbCurTable, col.name);
                                    const step = e.key === 'ArrowRight' ? 10 : -10;
                                    const curW = dbColWidths[key] ?? 120;
                                    dbColWidths = { ...dbColWidths, [key]: Math.max(40, curW + step) };
                                  }}
                                ></span>
                              </th>
                            {/if}
                          {/each}
                        </tr></thead>
                        <tbody>
                          {#each dbTableData.rows as row}
                            {@const rid = row.rowid ? parseInt(String(row.rowid)) : '—'}
                            <tr onclick={() => {
                              dbDetailRow = Object.fromEntries(Object.entries(row).filter(([k]) => k !== 'rowid'));
                              dbDetailRowId = parseInt(String(row.rowid ?? '0')) || 0;
                            }} class:dbm-row-selected={selectMode && typeof rid === 'number' && dbSelectedRows.has(rid)}>
                              <td class="dbm-rid">{rid}</td>
                              {#if selectMode}
                                <td class="dbm-cell dbm-sel-cell" onclick={(e) => e.stopPropagation()}>
                                  <Checkbox checked={typeof rid === 'number' && dbSelectedRows.has(rid)}
                                    onCheckedChange={() => typeof rid === 'number' && toggleRowSelect(rid)} />
                                </td>
                              {/if}
                              {#each dbTableData.columns as col (col.name)}
                                {#if dbVisibleCols?.has(col.name)}
                                  <td class="dbm-cell" title={fmtTsValue(row[col.name], col.name)
                                    ? `${String(row[col.name])}  (${fmtTsValue(row[col.name], col.name)})`
                                    : (String(row[col.name] ?? ''))}>
                                    {#if row[col.name] !== null && row[col.name] !== undefined}
                                      <span class="dbm-val">{row[col.name]}</span>
                                    {:else}
                                      <span class="dbm-null">NULL</span>
                                    {/if}
                                  </td>
                                {/if}
                              {/each}
                            </tr>
                          {/each}
                        </tbody>
                      </table>
                    {:else if !dbCurTable}
                      <div class="dbm-empty"><span class="dbm-empty-ico"><Table2Icon class="size-6" /></span>从左侧选择一张表开始浏览</div>
                    {:else if dbTableError}
                      <div class="dbm-empty dbm-empty-err"><span class="dbm-empty-ico"><XCircleIcon class="size-6" /></span>加载失败：{dbTableError}</div>
                    {:else}
                      <div class="dbm-empty"><span class="dbm-empty-ico"><LoaderCircleIcon class="size-6 dbm-spin" /></span>加载中...</div>
                    {/if}
                  </div>
                  {#if dbCurTable && dbTableData}
                    <div class="dbm-pager">
                      <span class="dbm-pager-total">共 <b>{fmtNum(dbTotal)}</b> 条{#if dbFilterText.trim()}（筛选后）{/if}</span>
                      <span class="dbm-pager-page">第 <b>{dbPage + 1}</b> / {dbTotalPages} 页</span>
                      <div class="dbm-pager-jump" title="输入页码后回车或点跳转">
                        <input type="number" min="1" max={dbTotalPages} bind:value={dbJumpPage}
                          onkeydown={(e) => e.key === 'Enter' && goToPage(dbJumpPage - 1)} />
                        <Button size="sm" variant="outline" class="h-7 px-2.5" onclick={() => goToPage(dbJumpPage - 1)} disabled={!dbCurTable}>跳</Button>
                      </div>
                      <SelectRoot type="single" value={String(dbPageSize)} onValueChange={(v) => { dbPageSize = parseInt(v); loadDbTableData(dbCurTable, 0); }}>
                        <SelectTrigger class="h-7 w-[92px] text-xs"><span>{dbPageSize} / 页</span></SelectTrigger>
                        <SelectContent>
                          {#each [50, 100, 200, 500, 1000] as n}
                            <SelectItem value={String(n)}>{n} / 页</SelectItem>
                          {/each}
                        </SelectContent>
                      </SelectRoot>
                      <div class="dbm-pager-nav">
                        <Button size="icon" variant="outline" class="h-7 w-7" disabled={dbPage === 0} onclick={() => goToPage(0)} title="首页">⏮</Button>
                        <Button size="sm" variant="outline" class="h-7 px-2.5" disabled={dbPage === 0 || !dbPrevCursor} onclick={() => loadDbTableData(dbCurTable, dbPage - 1, 'prev')}>‹ 上一页</Button>
                        <Button size="sm" variant="outline" class="h-7 px-2.5" disabled={(dbTotal > 0 && dbPage >= dbTotalPages - 1) || !dbNextCursor} onclick={() => loadDbTableData(dbCurTable, dbPage + 1, 'next')}>下一页 ›</Button>
                        <Button size="icon" variant="outline" class="h-7 w-7" disabled={dbPage >= dbTotalPages - 1} onclick={() => goToPage(dbTotalPages - 1)} title="末页">⏭</Button>
                      </div>
                    </div>
                  {/if}

                {:else if dbSubTab === 'schema'}
                  <div class="dbm-schema">
                    {#if !dbCurTable}
                      <div class="dbm-empty"><span class="dbm-empty-ico"><Table2Icon class="size-6" /></span>从左侧选择一张表查看结构</div>
                    {:else if dbSchemaLoading}
                      <div class="dbm-empty"><span class="dbm-empty-ico"><LoaderCircleIcon class="size-6 dbm-spin" /></span>加载中...</div>
                    {:else}
                      <div class="dbm-schema-card">
                        <div class="dbm-schema-hd">
                          <span class="dbm-schema-ico">▤</span>
                          <b>{dbCurTable}</b>
                          <span class="dbm-schema-count">{dbSchemaInfo.length} 个字段</span>
                          <button class="dbm-btn dbm-btn-xs" style="margin-left:auto" onclick={openCompare} title="与其它库/表对比结构">🔁 对比</button>
                        </div>
                        <table class="dbm-schema-grid">
                          <thead><tr><th style="width:44px">#</th><th>字段名</th><th>类型</th><th>约束</th><th>默认值</th></tr></thead>
                          <tbody>
                            {#each dbSchemaInfo as col, i}
                              <tr>
                                <td class="dbm-schema-idx">{i + 1}</td>
                                <td class="dbm-schema-name">{col.name}</td>
                                <td><span class="dbm-type-badge">{col.col_type || '—'}</span></td>
                                <td>
                                  {#if col.pk}<span class="dbm-tag dbm-tag-pk">PK</span>{/if}
                                  {#if col.not_null}<span class="dbm-tag dbm-tag-nn">NOT NULL</span>{/if}
                                  {#if !col.pk && !col.not_null}<span class="dbm-muted-text">—</span>{/if}
                                </td>
                                <td class="dbm-schema-default">{col.default ?? '—'}</td>
                              </tr>
                            {/each}
                          </tbody>
                        </table>
                      </div>
                      {#if tableDetailLoading}
                        <div class="dbm-empty" style="min-height:80px">加载详情中...</div>
                      {:else if tableDetail}
                        <div class="dbm-pills">
                          <button class="dbm-pill" class:dbm-pill-active={dbSchemaTab === 'indexes'} onclick={() => dbSchemaTab = 'indexes'}>索引<span class="dbm-pill-count">{tableDetail.indexes?.length ?? 0}</span></button>
                          <button class="dbm-pill" class:dbm-pill-active={dbSchemaTab === 'triggers'} onclick={() => dbSchemaTab = 'triggers'}>触发器<span class="dbm-pill-count">{tableDetail.triggers?.length ?? 0}</span></button>
                          <button class="dbm-pill" class:dbm-pill-active={dbSchemaTab === 'fk'} onclick={() => dbSchemaTab = 'fk'}>外键<span class="dbm-pill-count">{tableDetail.foreign_keys?.length ?? 0}</span></button>
                          <button class="dbm-pill" class:dbm-pill-active={dbSchemaTab === 'ddl'} onclick={() => dbSchemaTab = 'ddl'}>DDL</button>
                        </div>
                        {#if dbSchemaTab === 'indexes'}
                          <div class="dbm-schema-card">
                            <div class="dbm-schema-hd"><b>索引</b><span class="dbm-schema-count">{tableDetail.indexes?.length ?? 0}</span></div>
                            {#if tableDetail.indexes?.length}
                              <table class="dbm-schema-grid">
                                <thead><tr><th>名称</th><th>唯一</th><th>来源</th><th>列</th></tr></thead>
                                <tbody>
                                  {#each tableDetail.indexes as ix}
                                    <tr>
                                      <td class="dbm-schema-name">{ix.name}</td>
                                       <td>{#if ix.unique}<CheckIcon class="size-3.5" />{:else}<span class="dg-null">—</span>{/if}</td>
                                      <td><span class="dbm-type-badge">{ix.origin}</span></td>
                                      <td class="dbm-schema-default">{(ix.columns ?? []).join(', ') || '—'}</td>
                                    </tr>
                                  {/each}
                                </tbody>
                              </table>
                            {:else}<div class="dbm-muted-text" style="padding:8px 0">无索引</div>{/if}
                          </div>
                        {:else if dbSchemaTab === 'triggers'}
                          <div class="dbm-schema-card">
                            <div class="dbm-schema-hd"><b>触发器</b><span class="dbm-schema-count">{tableDetail.triggers?.length ?? 0}</span></div>
                            {#if tableDetail.triggers?.length}
                              {#each tableDetail.triggers as tg}
                                <details class="dbm-details">
                                  <summary>{tg.name}</summary>
                                  <pre class="dbm-ddl">{tg.sql || '—'}</pre>
                                </details>
                              {/each}
                            {:else}<div class="dbm-muted-text" style="padding:8px 0">无触发器</div>{/if}
                          </div>
                        {:else if dbSchemaTab === 'fk'}
                          <div class="dbm-schema-card">
                            <div class="dbm-schema-hd"><b>外键引用</b><span class="dbm-schema-count">{tableDetail.foreign_keys?.length ?? 0}</span></div>
                            <div class="dbm-fk-chips">
                              {#each tableDetail.foreign_keys ?? [] as fk}<span class="dbm-tag dbm-tag-fk">→ {fk}</span>{/each}
                              {#if !tableDetail.foreign_keys?.length}<div class="dbm-muted-text" style="padding:8px 0">无外键引用</div>{/if}
                            </div>
                          </div>
                        {:else}
                          <div class="dbm-schema-card">
                            <div class="dbm-schema-hd">
                              <b>建表 DDL</b>
                              <button class="dbm-btn dbm-btn-xs" onclick={copyDdl} disabled={!tableDetail.ddl} title="复制建表语句">⧉ 复制</button>
                              <button class="dbm-btn dbm-btn-xs" onclick={() => openDdlModal()} title="在新弹窗中查看 DDL">放大</button>
                            </div>
                            <pre class="dbm-ddl">{tableDetail.ddl || '（虚拟表无 DDL）'}</pre>
                          </div>
                        {/if}
                      {/if}
                    {/if}
                  </div>

                {:else if dbSubTab === 'sql'}
                  <div class="dbm-sql">
                    <div class="dbm-sql-bar">
                      <code class="dbm-sql-src">{extDbSelectedName}{extDbSelectedPath ? '（只读）' : '（可写）'}</code>
                      <Button size="sm" variant="outline" onclick={insertSqlSample} title="插入示例查询">示例</Button>
                      <Button size="sm" variant="outline" onclick={() => sqlText = ''} disabled={!sqlText}>清空</Button>
                    </div>
                    <Textarea class="dbm-sql-input" bind:value={sqlText} spellcheck="false"
                      onkeydown={(e) => { if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); runSql(); } }}
                      placeholder={'SELECT * FROM 表名 LIMIT 100\n外部数据库只读：仅允许 SELECT / WITH / PRAGMA / EXPLAIN / VALUES'}></Textarea>
                      <div class="dbm-sql-bar">
                        <Button onclick={runSql} disabled={sqlLoading || !sqlText.trim()}>
                          {sqlLoading ? '执行中...' : '▶ 执行'}
                        </Button>
                        <span class="dbm-muted-text">Ctrl+Enter 快速执行</span>
                        {#if extDbSelectedPath}<span class="dbm-muted-text">外部库只读</span>{/if}
                    </div>
                    {#if sqlError}
                      <div class="dbm-empty dbm-empty-err">SQL 执行失败：{sqlError}</div>
                    {/if}
                    {#if sqlResult}
                      {#if sqlResult.kind === 'write'}
                         <div class="dbm-status-ok"><CheckIcon class="size-3.5" /> 执行成功，影响 {sqlResult.affected} 行</div>
                      {:else}
                        <div class="dbm-sql-meta">返回 {sqlResult.rows.length} 行{#if sqlResult.truncated}（已达 500 行上限，已截断）{/if}</div>
                        <div class="dbm-grid-wrap dbm-sql-grid">
                          <table class="dbm-grid">
                            <thead><tr>
                              <th class="dbm-th dbm-rid-th">#</th>
                              {#each sqlResult.columns as c}<th class="dbm-th">{c}</th>{/each}
                            </tr></thead>
                            <tbody>
                              {#each sqlResult.rows as row, ri}
                                <tr>
                                  <td class="dbm-rid">{ri + 1}</td>
                                  {#each sqlResult.columns as c}
                                    <td class="dbm-cell" title={String(row[c] ?? '')}>
                                      {#if row[c] !== null && row[c] !== undefined}
                                        <span class="dbm-val">{row[c]}</span>
                                      {:else}
                                        <span class="dbm-null">NULL</span>
                                      {/if}
                                    </td>
                                  {/each}
                                </tr>
                              {/each}
                            </tbody>
                          </table>
                        </div>
                      {/if}
                    {/if}
                  </div>

                {:else}
                  <div class="dbm-status">
                    <div class="dbm-stat-grid">
                      <div class="dbm-stat-card">
                        <span class="dbm-stat-ico"><Table2Icon class="size-5" /></span>
                        <div class="dbm-stat-v">{dbTables.length}</div>
                        <div class="dbm-stat-l">数据表</div>
                      </div>
                      <div class="dbm-stat-card">
                        <span class="dbm-stat-ico"><SaveIcon class="size-5" /></span>
                        <div class="dbm-stat-v">{dbStatusInfo ? fmtBytes(dbStatusInfo.dbSize) : '—'}</div>
                        <div class="dbm-stat-l">文件大小</div>
                      </div>
                      <div class="dbm-stat-card">
                        <span class="dbm-stat-ico"><DatabaseIcon class="size-5" /></span>
                        <div class="dbm-stat-v">{extDbFiles.length + 1}</div>
                        <div class="dbm-stat-l">数据源文件</div>
                      </div>
                      <div class="dbm-stat-card">
                        <span class="dbm-stat-ico"><HashIcon class="size-5" /></span>
                        <div class="dbm-stat-v">{dbCurTable && dbTableData ? fmtNum(dbTotal) : '—'}</div>
                        <div class="dbm-stat-l">当前表记录数</div>
                      </div>
                    </div>
                    {#if dbStatusInfo}
                      <div class="dbm-status-path"><b>路径：</b>{dbStatusInfo.dbPath}</div>
                    {/if}

                    <div class="dbm-status-grid">
                      {#if !extDbSelectedPath}
                        <div class="dbm-status-card">
                        <div class="dbm-schema-hd"><b>内置库信息</b><button class="dbm-btn dbm-btn-xs" onclick={refreshInternalInfo}>刷新</button></div>
                        {#if internalDbInfo}
                          <div class="dbm-status-kv">
                            <span>路径</span><code>{internalDbInfo.path}</code>
                            <span>文件大小</span><code>{fmtBytes(internalDbInfo.size_bytes)}</code>
                            <span>事件</span><code>{fmtNum(internalDbInfo.event_count)}</code>
                            <span>任务</span><code>{fmtNum(internalDbInfo.task_count)}</code>
                            <span>Agent 日志</span><code>{fmtNum(internalDbInfo.agent_log_count)}</code>
                          </div>
                        {:else}<div class="dbm-muted-text" style="padding:8px 0">未加载</div>{/if}
                        <div class="dbm-status-actions">
                          <button class="dbm-btn" onclick={backupDb} disabled={backupBusy}>{backupBusy ? '备份中...' : '💾 备份数据库'}</button>
                          <button class="dbm-btn" onclick={restoreDb} title="选择备份文件，生成可替换的恢复文件">♻ 从备份恢复</button>
                          <button class="dbm-btn" onclick={triggerCleanup} disabled={cleanupBusy} title="按保留天数清理事件与 Agent 日志">
                            {cleanupBusy ? '清理中...' : `🧹 清理旧数据（${retentionDays} 天）`}
                          </button>
                        </div>
                        {#if cleanupResult}
                          <div class="dbm-status-ok">已清理 {cleanupResult.deleted_events} 条事件、{cleanupResult.deleted_agent} 条日志</div>
                        {/if}
                        {#if restoreHint}
                          <div class="dbm-status-hint">⚠️ {restoreHint}</div>
                        {/if}
                        </div>
                        <div class="dbm-status-card">
                          <div class="dbm-schema-hd"><b>最近事件</b><button class="dbm-btn dbm-btn-xs" onclick={() => loadEvents()}>刷新</button></div>
                        <div class="dbm-events">
                          {#each dbEvents as ev}
                            <div class="dbm-event">
                              <span class="dbm-event-time">{ev.timestamp}</span>
                              <span class="dbm-event-ty">{ev.event_type}</span>
                              <span class="dbm-event-title">{ev.title}</span>
                            </div>
                          {/each}
                          {#if dbEvents.length === 0}<div class="dbm-muted-text">暂无事件</div>{/if}
                        </div>
                        </div>
                      {:else}
                        <div class="dbm-status-card">
                        <div class="dbm-schema-hd"><b>文件诊断</b><button class="dbm-btn dbm-btn-xs" onclick={diagnoseDb} disabled={diagBusy}>{diagBusy ? '诊断中...' : '重新诊断'}</button></div>
                        {#if dbDiag}
                          {#if 'error' in dbDiag}
                            <div class="dbm-empty dbm-empty-err">{dbDiag.error}</div>
                          {:else}
                            <div class="dbm-status-kv">
                               <span>是否 SQLite</span><code>{#if dbDiag.is_sqlite}<CheckIcon class="size-3.5" /> 是{:else}<XCircleIcon class="size-3.5" /> 否{/if}</code>
                              <span>文件大小</span><code>{fmtBytes(dbDiag.size_bytes)}</code>
                              <span>页数（估算）</span><code>{fmtNum(dbDiag.page_count)}</code>
                              <span>文件头</span><code class="dbm-diag-hex">{dbDiag.header_text || ''}</code>
                            </div>
                          {/if}
                        {:else}<div class="dbm-muted-text" style="padding:8px 0">未诊断，点击「重新诊断」</div>{/if}
                        </div>
                      {/if}

                      <div class="dbm-status-card">
                        <div class="dbm-schema-hd">
                          <b>完整性检查</b>
                          <button class="dbm-btn dbm-btn-xs" onclick={runIntegrity} disabled={integrityBusy}>{integrityBusy ? '检查中...' : '开始检查'}</button>
                        </div>
                        {#if integrityResult}
                          {#if 'error' in integrityResult}
                            <div class="dbm-empty dbm-empty-err">{integrityResult.error}</div>
                          {:else}
                            <div class="dbm-integrity">
                              <div class="dbm-integrity-hd">integrity_check</div>
                              {#each integrityResult.integrity ?? [] as line}
                                <div class:dbm-integrity-ok={line === 'ok'}>{line}</div>
                              {/each}
                              {#if (integrityResult.foreign_keys ?? []).length > 0}
                                <div class="dbm-integrity-hd" style="margin-top:8px">外键违规 {integrityResult.foreign_keys.length} 条</div>
                                {#each integrityResult.foreign_keys as fk}
                                  <div>{fk.table} (rowid {fk.rowid}) → {fk.parent}</div>
                                {/each}
                              {/if}
                            </div>
                          {/if}
                        {/if}
                      </div>
                    </div>
                  </div>
                {/if}
              </div>
            </main>
          </div>
          <!-- 记录详情：右侧抽屉检查器 -->
          <SheetRoot open={dbDetailRow !== null} onOpenChange={(o) => !o && (dbDetailRow = null)}>
            <SheetContent side="right" class="flex w-[480px] max-w-[92vw] flex-col gap-0 p-0 sm:max-w-[480px]">
              <SheetHeader class="border-b px-5 py-4">
                <SheetTitle class="flex items-center gap-2 text-sm">{dbCurTable}</SheetTitle>
                <SheetDescription>rowid = {dbDetailRowId || '—'}{#if !isInternalDb} · 外部库只读{/if}</SheetDescription>
              </SheetHeader>
              <div class="flex-1 overflow-y-auto p-4">
                <div class="dbm-insp" role="list">
                  {#each Object.entries(dbDetailRow ?? {}) as [key, val]}
                    <div class="dbm-insp-field">
                      <div class="dbm-insp-field-hd">
                        <span class="dbm-insp-field-name" title={key}>{key}</span>
                        {#if fmtTsValue(val, key)}<span class="dbm-insp-ts"><Clock3Icon class="size-3.5" /> {fmtTsValue(val, key)}</span>{/if}
                        <span class="dbm-insp-actions">
                          <button class="dbm-table-act" onclick={() => copyField(key, val)} title="复制字段值"><CopyIcon class="size-3.5" /></button>
                          {#if isBlobPreview(String(val ?? ''))}
                            <button class="dbm-table-act" onclick={() => openBlobViewer(String(key))} title="查看原始内容"><EyeIcon class="size-3.5" /></button>
                          {/if}
                        </span>
                      </div>
                      <div class="dbm-insp-field-val">
                        {#if val !== null && val !== undefined}
                          {val}
                        {:else}
                          <span class="dg-null">NULL</span>
                        {/if}
                      </div>
                    </div>
                  {/each}
                </div>
              </div>
              <div class="flex items-center gap-2 border-t px-5 py-3">
                <button class="dbm-btn" onclick={copyRowDetail} title="复制整行为 JSON"><ClipboardCopyIcon class="size-3.5" /> 复制整行</button>
                {#if isInternalDb && dbDetailRowId}
                  <button class="dbm-btn" onclick={openEditRow} title="编辑该行（仅内置数据库）"><PencilIcon class="size-3.5" /> 编辑</button>
                  <button class="dbm-btn dbm-btn-danger" onclick={requestDeleteRow} title="删除该行（仅内置数据库）"><Trash2Icon class="size-3.5" /> 删除</button>
                {/if}
                <div style="flex:1"></div>
                <Button size="sm" onclick={() => (dbDetailRow = null)}>关闭</Button>
              </div>
            </SheetContent>
          </SheetRoot>

          <!-- 新增 / 编辑行弹窗 -->
          <DialogRoot open={crudMode !== null} onOpenChange={(o) => !o && !crudSaving && (crudMode = null)}>
            <DialogContent class="max-w-lg">
              <DialogHeader>
                <DialogTitle class="flex items-center gap-2">
                  <span>{crudMode === 'insert' ? '新增行' : '编辑行'}</span>
                  <span class="text-xs font-normal text-muted-foreground">{dbCurTable}</span>
                  {#if !isInternalDb}<span class="dbm-badge">外部库只读</span>{/if}
                </DialogTitle>
              </DialogHeader>
              <div class="max-h-[60vh] overflow-auto">
                  {#if crudColumns.length === 0}
                    <div class="dbm-empty">该表没有可编辑字段（仅含 rowid 或 BLOB 列）</div>
                  {:else}
                    <div class="dbm-crud-form">
                      {#each crudColumns as c}
                        <div class="dbm-crud-field">
                          <div class="dbm-crud-label" title={c.col_type || ''}>
                            {c.name}
                            <span class="dbm-coli-ty">{c.col_type || ''}</span>
                            {#if c.not_null}<span class="dbm-tag dbm-tag-nn">NOT NULL</span>{/if}
                          </div>
                          <div class="dbm-crud-input-row">
                            <input type={/INT|REAL|NUMERIC|DOUBLE|FLOAT/i.test(c.col_type || '') ? 'number' : 'text'} step="any" bind:value={crudValues[c.name]} disabled={crudNulls[c.name]}
                              placeholder={c.not_null ? '必填，留空会报错' : '留空视为 NULL'} />
                            <label class="dbm-crud-null">
                              <input type="checkbox" bind:checked={crudNulls[c.name]} disabled={c.not_null} /> NULL
                            </label>
                          </div>
                        </div>
                      {/each}
                      {#if crudError}
                        <div class="dbm-empty dbm-empty-err">操作失败：{crudError}</div>
                      {/if}
                    </div>
                  {/if}
              </div>
              <DialogFooter>
                <Button variant="outline" onclick={() => (crudMode = null)} disabled={crudSaving}>取消</Button>
                <Button onclick={saveCrudRow} disabled={crudSaving || crudColumns.length === 0}>
                  {crudSaving ? '保存中...' : crudMode === 'insert' ? '插入' : '保存修改'}
                </Button>
              </DialogFooter>
            </DialogContent>
          </DialogRoot>

          <!-- 删除确认弹窗 -->
          <DialogRoot open={deleteTarget !== null} onOpenChange={(o) => !o && !deleteSaving && (deleteTarget = null)}>
            <DialogContent class="max-w-md">
              <DialogHeader>
                <DialogTitle>确认删除</DialogTitle>
              </DialogHeader>
              <div>
                  <div class="dbm-delete-warn">
                    <p>确定删除 <b>{dbCurTable}</b> 中 rowid = <b>{deleteTarget?.rowid}</b> 的这行数据吗？</p>
                    <p class="dbm-delete-warn-sub">此操作不可撤销，且仅影响本机数据。</p>
                  </div>
              </div>
              <DialogFooter>
                <Button variant="outline" onclick={() => (deleteTarget = null)} disabled={deleteSaving}>取消</Button>
                <Button variant="destructive" onclick={confirmDeleteRow} disabled={deleteSaving}>{deleteSaving ? '删除中...' : '确认删除'}</Button>
              </DialogFooter>
            </DialogContent>
          </DialogRoot>

          <!-- 原始内容查看器 -->
          <DialogRoot open={blobViewer !== null} onOpenChange={(o) => !o && (blobViewer = null)}>
            <DialogContent class="max-w-3xl">
              <DialogHeader>
                <DialogTitle class="flex flex-wrap items-center gap-2">
                  <span class="dbm-insp-field-name" title={`${dbCurTable}.${blobViewer?.column}`}>{dbCurTable}.{blobViewer?.column}</span>
                  {#if blobViewer?.data?.kind === 'blob'}
                    <span class="dbm-badge">{blobViewer.data.mime}</span>
                    <span class="dbm-badge">{fmtBytes(blobViewer.data.length)}</span>
                  {/if}
                  <span class="flex-1"></span>
                  {#if blobViewer?.data?.kind === 'blob' && blobViewer.data.is_image}
                    <div class="dbm-pills" style="margin:0">
                      <button class="dbm-pill" class:dbm-pill-active={blobTab === 'preview'} onclick={() => blobTab = 'preview'}>预览</button>
                      <button class="dbm-pill" class:dbm-pill-active={blobTab === 'hex'} onclick={() => blobTab = 'hex'}>Hex</button>
                    </div>
                  {/if}
                </DialogTitle>
              </DialogHeader>
              <div class="max-h-[62vh] overflow-auto">
                  {#if blobLoading}
                    <div class="dbm-empty" style="min-height:120px">加载中...</div>
                  {:else if blobViewer?.data?.kind === 'blob' && blobViewer.data.is_image && blobTab === 'preview'}
                    <img src={blobDataUrl(blobViewer.data)} alt="BLOB 图片预览" style="max-width:100%;max-height:60vh;display:block;margin:0 auto;border-radius:8px;object-fit:contain;background:repeating-conic-gradient(#ffffff14 0 25%,transparent 0 50%) 0 0/16px 16px" />
                  {:else if blobViewer?.data?.kind === 'blob'}
                    <div class="blob-hex">{blobViewer.data.hex_preview}{#if blobViewer.data.length > 256}<div style="margin-top:8px;color:var(--dg-muted);font-size:12px">仅显示前 256 字节，完整内容请点击「⤓ 下载」</div>{/if}</div>
                  {:else if blobViewer?.data?.kind === 'text'}
                    <pre class="blob-hex" style="white-space:pre-wrap;word-break:break-all">{blobViewer.data.text}</pre>
                  {:else if blobViewer?.data?.kind === 'error'}
                    <div class="dbm-empty dbm-empty-err">读取失败：{blobViewer.data.text}</div>
                  {:else}
                    <div class="dbm-empty">NULL</div>
                  {/if}
              </div>
              <DialogFooter>
                <Button variant="outline" onclick={downloadBlob} disabled={blobViewer?.data?.kind !== 'blob'}>⤓ 下载</Button>
                <Button onclick={() => (blobViewer = null)}>关闭</Button>
              </DialogFooter>
            </DialogContent>
          </DialogRoot>

          <!-- 表操作菜单 -->
          <!-- 建表 DDL 弹窗 -->
          <DialogRoot open={ddlModalOpen} onOpenChange={(o) => !o && (ddlModalOpen = false)}>
            <DialogContent class="max-w-2xl">
              <DialogHeader>
                <DialogTitle class="flex items-center gap-2">
                  <span>建表 DDL</span>
                  <span class="text-xs font-normal text-muted-foreground">{dbCurTable}</span>
                </DialogTitle>
              </DialogHeader>
              <pre class="blob-hex dbm-ddl max-h-[55vh] overflow-auto">{tableDetail?.ddl || '（虚拟表无 DDL）'}</pre>
              <DialogFooter>
                <Button variant="outline" onclick={copyDdl} disabled={!tableDetail?.ddl} title="复制建表语句">⧉ 复制</Button>
                <Button onclick={() => (ddlModalOpen = false)}>关闭</Button>
              </DialogFooter>
            </DialogContent>
          </DialogRoot>

          <!-- 列统计弹窗 -->
          <DialogRoot open={statsOpen} onOpenChange={(o) => !o && (statsOpen = false)}>
            <DialogContent class="max-w-3xl">
              <DialogHeader>
                <DialogTitle class="flex items-center gap-2">
                  <span>列统计</span>
                  <span class="text-xs font-normal text-muted-foreground">{dbCurTable} · 抽样 2000 行</span>
                </DialogTitle>
              </DialogHeader>
              <div class="max-h-[60vh] overflow-auto">
                  {#if statsLoading}
                    <div class="dbm-empty">统计中...</div>
                  {:else if statsData && 'error' in statsData}
                    <div class="dbm-empty dbm-empty-err">{statsData.error}</div>
                  {:else if statsData}
                    <div class="dbm-stats-grid">
                      {#each statsData.columns as c}
                        <div class="dbm-stat-col">
                          <div class="dbm-stat-col-hd"><b>{c.name}</b><span class="dbm-type-badge">{c.type || '—'}</span></div>
                          <div class="dbm-stat-row">样本 {fmtNum(c.sample)} · 非空 {fmtNum(c.non_null)} · NULL {fmtNum(c.null_count)}（{c.null_pct}%）</div>
                          {#if c.is_numeric}<div class="dbm-stat-row">min {c.min ?? '—'} · max {c.max ?? '—'} · sum {c.sum ?? '—'}</div>{/if}
                          {#if c.top?.length}
                            <div class="dbm-stat-tops">
                              {#each c.top as t}<span class="dbm-tag" title={`${t.count} 次`}>{t.value} ×{t.count}</span>{/each}
                            </div>
                          {/if}
                        </div>
                      {/each}
                    </div>
                  {/if}
              </div>
              <DialogFooter>
                <Button onclick={() => (statsOpen = false)}>关闭</Button>
              </DialogFooter>
            </DialogContent>
          </DialogRoot>

          <!-- 表结构对比弹窗 -->
          <DialogRoot open={compareOpen} onOpenChange={(o) => !o && (compareOpen = false)}>
            <DialogContent class="max-w-5xl">
              <DialogHeader>
                <DialogTitle>表结构对比</DialogTitle>
              </DialogHeader>
              <div class="max-h-[62vh] overflow-auto">
                  <div class="dbm-cmp-grid">
                    <div class="dbm-cmp-side">
                      <div class="dbm-cmp-label">数据源 A</div>
                      <SelectRoot type="single" value={compareSrcA} onValueChange={(v) => { compareSrcA = v; onCompareSrcChanged('A'); }}>
                        <SelectTrigger class="w-full"><span>{compareSrcA || '选择数据源'}</span></SelectTrigger>
                        <SelectContent>
                          {#each compareSrcOptions() as o}<SelectItem value={o.value}>{o.label}</SelectItem>{/each}
                        </SelectContent>
                      </SelectRoot>
                      <SelectRoot type="single" value={compareTableA} onValueChange={(v) => (compareTableA = v)}>
                        <SelectTrigger class="w-full"><span>{compareTableA || '选择表'}</span></SelectTrigger>
                        <SelectContent>
                          {#each compareTablesA as t}<SelectItem value={t}>{t}</SelectItem>{/each}
                        </SelectContent>
                      </SelectRoot>
                    </div>
                    <div class="dbm-cmp-side">
                      <div class="dbm-cmp-label">数据源 B</div>
                      <SelectRoot type="single" value={compareSrcB} onValueChange={(v) => { compareSrcB = v; onCompareSrcChanged('B'); }}>
                        <SelectTrigger class="w-full"><span>{compareSrcB || '选择数据源'}</span></SelectTrigger>
                        <SelectContent>
                          {#each compareSrcOptions() as o}<SelectItem value={o.value}>{o.label}</SelectItem>{/each}
                        </SelectContent>
                      </SelectRoot>
                      <SelectRoot type="single" value={compareTableB} onValueChange={(v) => (compareTableB = v)}>
                        <SelectTrigger class="w-full"><span>{compareTableB || '选择表'}</span></SelectTrigger>
                        <SelectContent>
                          {#each compareTablesB as t}<SelectItem value={t}>{t}</SelectItem>{/each}
                        </SelectContent>
                      </SelectRoot>
                    </div>
                  </div>
                  <div class="dbm-sql-bar" style="justify-content:flex-end;margin:10px 0">
                    <Button onclick={doCompare} disabled={compareLoading}>{compareLoading ? '对比中...' : '开始对比'}</Button>
                  </div>
                  {#if compareResult}
                    {#if 'error' in compareResult}
                      <div class="dbm-empty dbm-empty-err">{compareResult.error}</div>
                    {:else}
                      <div class="dbm-modal-stack">
                      <div class="dbm-schema-card">
                        <div class="dbm-schema-hd"><b>仅 A 有</b><span class="dbm-schema-count">{compareResult.onlyA.length}</span></div>
                        {#if compareResult.onlyA.length}
                          <div class="dbm-cmp-chips">{#each compareResult.onlyA as c}<span class="dbm-tag dbm-tag-fk">{c.name} ({c.col_type})</span>{/each}</div>
                        {:else}<div class="dbm-muted-text" style="padding:8px 0">无</div>{/if}
                      </div>
                      <div class="dbm-schema-card">
                        <div class="dbm-schema-hd"><b>仅 B 有</b><span class="dbm-schema-count">{compareResult.onlyB.length}</span></div>
                        {#if compareResult.onlyB.length}
                          <div class="dbm-cmp-chips">{#each compareResult.onlyB as c}<span class="dbm-tag dbm-tag-fk">{c.name} ({c.col_type})</span>{/each}</div>
                        {:else}<div class="dbm-muted-text" style="padding:8px 0">无</div>{/if}
                      </div>
                      <div class="dbm-schema-card">
                        <div class="dbm-schema-hd"><b>类型变化</b><span class="dbm-schema-count">{compareResult.changed.length}</span></div>
                        {#if compareResult.changed.length}
                          <table class="dbm-schema-grid">
                            <thead><tr><th>字段</th><th>A 类型</th><th>B 类型</th></tr></thead>
                            <tbody>
                              {#each compareResult.changed as c}
                                <tr><td class="dbm-schema-name">{c.name}</td><td><span class="dbm-type-badge">{c.a}</span></td><td><span class="dbm-type-badge">{c.b}</span></td></tr>
                              {/each}
                            </tbody>
                          </table>
                        {:else}<div class="dbm-muted-text" style="padding:8px 0">无类型变化</div>{/if}
                      </div>
                      </div>
                    {/if}
                  {/if}
                </div>
              <DialogFooter>
                <Button onclick={() => (compareOpen = false)}>关闭</Button>
              </DialogFooter>
            </DialogContent>
          </DialogRoot>

          <!-- 字段类型悬浮提示 -->
          <div class="dbm-coltip" class:dbm-coltip-show={dbColTip !== null}
            style={dbColTip ? `left:${dbColTip.x}px;top:${dbColTip.y}px` : 'left:-9999px;top:-9999px;opacity:0'}>
            {#if dbColTip}
              <div class="dbm-coltip-name">{dbColTip.name}</div>
              <div class="dbm-coltip-type">{dbColTip.type}</div>
            {/if}
          </div>
        </div>


<style>
  .dbm {
    /* dg 令牌 → 全局 shadcn 令牌（此前从未定义，导致全部样式失效） */
    --dg-card: var(--card);
    --dg-card-hover: var(--muted);
    --dg-card-active: color-mix(in oklab, var(--accent) 58%, var(--card));
    --dg-border: var(--border);
    --dg-border-light: color-mix(in oklab, var(--border) 55%, transparent);
    --dg-text: var(--foreground);
    --dg-text2: var(--muted-foreground);
    --dg-muted: var(--muted-foreground);
    --dg-accent: var(--primary);
    --dg-bg-muted: var(--muted);
  }
  /* ── 通用按钮 ── */
  .dbm-btn {
    display:inline-flex; align-items:center; gap:4px; padding:6px 12px;
    border:1px solid var(--dg-border); border-radius:8px; background:var(--dg-card);
    color:var(--dg-text2); font-size:12px; cursor:pointer; transition:all .15s; white-space:nowrap;
    font-family:inherit;
  }
  .dbm-btn:hover:not(:disabled) { background:var(--dg-card-hover); color:var(--dg-text); border-color:var(--dg-accent); }
  .dbm-btn:disabled { opacity:.45; cursor:not-allowed; }
  .dbm-btn-xs { padding:2px 8px; font-size:11.5px; }

  /* ── 根布局 ── */
  .dbm { flex:1; min-height:0; display:flex; flex-direction:column; overflow:hidden; }

  /* ── 顶部标题栏 ── */
  .dbm-header {
    display:flex; align-items:center; justify-content:space-between; gap:12px;
    padding:10px 0; border-bottom:1px solid var(--dg-border-light); flex-shrink:0;
  }
  .dbm-title { display:flex; align-items:center; gap:10px; min-width:0; }
  .dbm-title-ico { font-size:18px; }
  .dbm-title h2 { margin:0; font-size:16px; font-weight:700; color:var(--dg-text); letter-spacing:-.3px; white-space:nowrap; }
  /* 顶栏数据源下拉 */
  .dbm-src-select-wrap { position:relative; }
  .dbm-actions { display:flex; gap:8px; flex-shrink:0; flex-wrap:wrap; }
  .dbm-badge { font-size:11.5px; padding:1px 8px; border-radius:999px; background:var(--dg-accent); color:var(--dg-text2); }

  /* ── 主体布局 ── */
  .dbm-body { flex:1; min-height:0; display:flex; gap:16px; padding-top:10px; overflow:hidden; }

  /* ── 左侧边栏 ── */
  .dbm-side { width:280px; flex-shrink:0; display:flex; flex-direction:column; gap:14px; min-height:0; overflow:hidden; transition:width .18s ease; }
  .dbm-side-collapsed { width:46px; }
  .dbm-side-collapsed .dbm-table-search,
  .dbm-side-collapsed .dbm-table-list,
  .dbm-side-collapsed .dbm-table-sec-hd,
  .dbm-side-collapsed .dbm-badge { display:none; }
  .dbm-side-collapsed .dbm-side-hd { justify-content:center; padding:4px 0; }
  .dbm-side-collapsed .dbm-side-hd span:first-child { display:none; }
  .dbm-side-sec { display:flex; flex-direction:column; min-height:0; }
  .dbm-side-sec-tables { flex:1; }
  .dbm-side-hd {
    display:flex; align-items:center; justify-content:space-between;
    padding:2px 8px 8px; font-size:12px; font-weight:600; color:var(--dg-text2); letter-spacing:.4px;
  }
  .dbm-mini { border:none; background:none; color:var(--dg-muted); cursor:pointer; font-size:12px; padding:2px 6px; border-radius:6px; }
  .dbm-mini:hover { background:var(--dg-card-hover); color:var(--dg-text); }
  @keyframes dbmSpin { to { transform:rotate(360deg); } }

  .dbm-table-search { padding:0 8px 8px; }
  .dbm-table-search-box { position:relative; display:flex; align-items:center; }
  .dbm-table-search-box .dbm-filter-clear { right:6px; }
  .dbm-link-btn {
    margin-left:6px; border:none; background:none; color:var(--dg-accent);
    cursor:pointer; font-size:11.5px; text-decoration:underline; padding:0;
  }
  .dbm-link-btn:hover { color:var(--dg-text); }
  .dbm-table-list { overflow-y:auto; scrollbar-width:thin; display:flex; flex-direction:column; gap:2px; min-height:0; }
  .dbm-table-ico { width:16px; text-align:center; flex-shrink:0; font-size:11.5px; opacity:.75; }
  .dbm-table-name { flex:1; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; font-family:ui-monospace,Consolas,monospace; font-size:12px; }
  .dbm-empty-mini { padding:10px 8px; font-size:12px; color:var(--dg-muted); text-align:center; }
  /* ── 主区域 ── */
  .dbm-main { flex:1; min-width:0; display:flex; flex-direction:column; min-height:0; }
  :global(.dbm-tabs) { flex-shrink:0; }
  :global(.dbm-tabs [data-slot="tabs-list"]) {
    width: fit-content;
    margin-bottom: 8px;
    border-bottom: 1px solid var(--dg-border-light);
  }
  .dbm-content { flex:1; min-height:0; display:flex; flex-direction:column; padding-top:10px; overflow:hidden; }

  /* ── 工具栏 ── */
  .dbm-toolbar { display:flex; align-items:center; gap:10px; padding-bottom:10px; flex-shrink:0; flex-wrap:wrap; }
  .dbm-toolbar-right { display:flex; align-items:center; gap:6px; flex-shrink:0; flex-wrap:wrap; margin-left:auto; }
  .dbm-tb-sep { width:1px; height:20px; background:var(--dg-border-light); flex-shrink:0; margin:0 2px; }
  .dbm-crumb { font-size:12px; color:var(--dg-muted); flex:1; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .dbm-crumb b { color:var(--dg-text2); font-weight:600; font-family:ui-monospace,Consolas,monospace; }
  .dbm-filter { position:relative; display:flex; align-items:center; }
  .dbm-filter-clear { position:absolute; right:4px; border:none; background:none; color:var(--dg-muted); cursor:pointer; display:inline-flex; padding:3px; border-radius:4px; }
  .dbm-filter-clear:hover { background:var(--dg-card-hover); color:var(--dg-text); }

  /* ── 列菜单 ── */
  .dbm-colmenu-wrap { position:relative; }
  .dbm-colmenu {
    position:absolute; right:0; top:calc(100% + 6px); z-index:60;
    width:280px; max-height:340px; overflow-y:auto; padding:10px;
    background:var(--dg-card); border:1px solid var(--dg-border); border-radius:12px;
    box-shadow:0 8px 24px color-mix(in srgb, var(--app-bg-color) 60%, transparent);
  }
  .dbm-colmenu-hd {
    display:flex; align-items:center; justify-content:space-between;
    padding-bottom:8px; border-bottom:1px solid var(--dg-border-light); margin-bottom:6px;
    font-size:12px; font-weight:600; color:var(--dg-text2);
  }
  .dbm-colmenu-body { display:flex; flex-direction:column; gap:1px; }
  .dbm-coli { display:flex; align-items:center; gap:8px; padding:5px 6px; border-radius:6px; cursor:pointer; font-size:12px; color:var(--dg-text2); }
  .dbm-coli:hover { background:var(--dg-card-hover); }
  .dbm-coli-ty { margin-left:auto; font-size:11.5px; color:var(--dg-muted); font-family:monospace; }

  /* ── 数据表格 ── */
  .dbm-grid-wrap {
    flex:1; min-height:0; overflow:auto; scrollbar-width:thin;
    border:1px solid var(--dg-border-light); border-radius:12px; background:var(--dg-card);
  }
  .dbm-grid { width:100%; border-collapse:separate; border-spacing:0; font-size:13px; }
  .dbm-grid thead th {
    position:sticky; top:0; z-index:2; background:var(--dg-card-active);
    color:var(--dg-text2); font-size:12px; font-weight:600; text-align:left;
  }
  .dbm-th { position:relative; padding:9px 12px; border-bottom:1px solid var(--dg-border); white-space:nowrap; }
  .dbm-rid-th { width:52px; text-align:center; padding-right:8px; }
  .dbm-th-sort { cursor:pointer; user-select:none; transition:background .15s; }
  .dbm-th-sort:hover { background:var(--dg-card-hover); color:var(--dg-text); }
  .dbm-th-name { max-width:260px; overflow:hidden; text-overflow:ellipsis; display:inline-block; vertical-align:bottom; }
  .dbm-arr { font-size:11.5px; margin-left:4px; }
  .dbm-resize { position:absolute; right:-1px; top:0; width:7px; height:100%; cursor:col-resize; z-index:3; }
  .dbm-resize:hover { background:color-mix(in srgb, var(--dg-accent) 40%, transparent); }
  .dbm-grid tbody tr { cursor:pointer; transition:background .1s, box-shadow .12s; content-visibility:auto; contain-intrinsic-size:auto 36px; }
  .dbm-grid tbody tr:nth-child(even) { background:color-mix(in srgb, var(--dg-card) 97%, var(--dg-text)); }
  .dbm-grid tbody tr:hover { background:var(--dg-card-hover); box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--dg-accent) 16%, transparent); }
  .dbm-grid td { padding:8px 12px; border-bottom:1px solid var(--dg-border-light); }
  .dbm-grid tbody tr:last-child td { border-bottom:none; }
  .dbm-cell { max-width:420px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .dbm-val { word-break:break-all; }
  .dbm-null { color:var(--dg-muted); font-style:italic; font-size:11.5px; }
  .dbm-rid { color:var(--dg-muted); text-align:center; font-size:11.5px; font-family:ui-monospace,Consolas,monospace; }
  /* ── 分页 ── */
  .dbm-pager {
    display:flex; align-items:center; flex-wrap:wrap; row-gap:8px; gap:14px; padding-top:12px; flex-shrink:0;
    font-size:12px; color:var(--dg-muted);
  }
  .dbm-pager b { color:var(--dg-text); font-weight:600; }
  .dbm-pager-nav { margin-left:auto; display:flex; gap:6px; }

  /* ── 空状态 ── */
  .dbm-empty {
    flex:1; min-height:160px; display:flex; align-items:center; justify-content:center;
    flex-direction:column; gap:10px; color:var(--dg-muted); font-size:14px;
  }
  .dbm-empty-ico { font-size:32px; opacity:.85; display:inline-flex; color:var(--dg-accent); }
  .dbm-empty-err { color:var(--dg-danger,#ff5252); }
  .dbm-muted-text { color:var(--dg-muted); }

  /* ── 表结构 ── */
  .dbm-schema { flex:1; min-height:0; overflow:auto; scrollbar-width:thin; display:flex; flex-direction:column; gap:12px; padding-right:4px; }
  .dbm-schema-card {
    border:1px solid var(--dg-border-light); border-radius:12px; background:var(--dg-card);
    overflow:hidden; max-width:100%;
  }
  .dbm-schema-hd {
    display:flex; align-items:center; gap:10px; padding:12px 16px;
    border-bottom:1px solid var(--dg-border-light); font-size:14px; color:var(--dg-text);
  }
  .dbm-schema-ico { font-size:14px; opacity:.8; }
  .dbm-schema-count { font-size:12px; color:var(--dg-muted); }
  .dbm-schema-grid { width:100%; border-collapse:collapse; font-size:13px; }
  .dbm-schema-grid th, .dbm-schema-grid td { padding:9px 16px; text-align:left; border-bottom:1px solid var(--dg-border-light); }
  .dbm-schema-grid thead th { font-size:12px; font-weight:600; color:var(--dg-text2); background:var(--dg-card-active); }
  .dbm-schema-grid tbody tr:last-child td { border-bottom:none; }
  .dbm-schema-idx { color:var(--dg-muted); font-size:11.5px; }
  .dbm-schema-name { font-family:ui-monospace,Consolas,monospace; color:var(--dg-text); }
  .dbm-type-badge {
    font-family:ui-monospace,Consolas,monospace; font-size:11.5px; padding:2px 8px;
    border-radius:6px; background:var(--dg-bg-muted); color:var(--dg-text2);
  }
  .dbm-tag { font-size:11.5px; padding:2px 7px; border-radius:999px; margin-right:4px; font-weight:600; }
  .dbm-tag-pk { background:rgba(255,193,7,.16); color:#d4a017; }
  .dbm-tag-nn { background:rgba(244,67,54,.13); color:#e57373; }
  .dbm-schema-default { color:var(--dg-muted); font-family:ui-monospace,Consolas,monospace; font-size:12px; }

  /* ── 状态概览 ── */
  .dbm-status { flex:1; min-height:0; overflow:auto; scrollbar-width:thin; }
  .dbm-stat-grid { display:grid; grid-template-columns:repeat(auto-fit,minmax(170px,1fr)); gap:12px; margin-bottom:14px; }
  .dbm-stat-card {
    padding:16px; border:1px solid var(--dg-border-light); border-radius:12px;
    background:var(--dg-card); display:flex; flex-direction:column; gap:6px;
    transition:background .15s, border-color .15s;
  }
  .dbm-stat-card:hover { background:var(--dg-card-hover); border-color:var(--dg-accent); }
  .dbm-stat-ico { font-size:20px; }
  .dbm-stat-v { font-size:26px; font-weight:700; color:var(--dg-text); letter-spacing:-.5px; }
  .dbm-stat-l { font-size:12px; color:var(--dg-muted); letter-spacing:.3px; }
  .dbm-status-path { font-size:12px; color:var(--dg-muted); word-break:break-all; padding:2px 0 12px; }
  /* ── 字段类型悬浮提示 ── */
  .dbm-coltip {
    position:fixed; z-index:2000; pointer-events:none;
    transform:translate(-50%, calc(-100% - 8px));
    background:var(--app-color-card-bg); border:1px solid var(--dg-border); border-radius:8px;
    padding:6px 10px; box-shadow:0 4px 14px color-mix(in srgb, var(--app-bg-color) 50%, transparent);
    font-size:11.5px; color:var(--dg-text); opacity:0; transition:opacity .15s; white-space:nowrap;
  }
  .dbm-coltip-show { opacity:1; }
  .dbm-coltip-name { font-weight:600; margin-bottom:2px; }
  .dbm-coltip-type { font-family:monospace; font-size:11.5px; color:var(--dg-muted); }

  /* ── 弹窗（详情 / BLOB 查看器） ── */
  @keyframes dbmOverlayIn { from{opacity:0} to{opacity:1} }
  @keyframes dbmModalIn {
    from{transform:translateY(24px) scale(.97);opacity:0}
    to{transform:translateY(0) scale(1);opacity:1}
  }
  .dg-null { color:var(--dg-muted); font-style:italic; }

  /* ── BLOB hex 预览 ── */
  .blob-hex {
    font-family:ui-monospace,Consolas,"Courier New",monospace;
    font-size:12px; line-height:1.6;
    background:var(--dg-bg-muted); border:1px solid var(--dg-border-light);
    border-radius:10px; padding:12px; overflow:auto;
    word-break:break-all; color:var(--dg-text2); max-height:55vh;
    white-space:pre-wrap;
  }

  /* ── CRUD 表单 / 弹窗底部 ── */
  .dbm-crud-form { display:flex; flex-direction:column; gap:12px; padding:2px 0; }
  .dbm-crud-field { display:flex; flex-direction:column; gap:4px; }
  .dbm-crud-label {
    display:flex; align-items:center; gap:6px;
    font-size:12px; font-weight:600; color:var(--dg-text2);
    font-family:ui-monospace,Consolas,monospace;
  }
  .dbm-crud-input-row { display:flex; align-items:center; gap:8px; }
  .dbm-crud-input-row input[type="text"] {
    flex:1; min-width:0; padding:7px 10px;
    border:1px solid var(--dg-border); border-radius:8px;
    background:var(--dg-card); color:var(--dg-text); font-size:12px; outline:none; font-family:inherit;
  }
  .dbm-crud-input-row input[type="text"]:focus { border-color:var(--dg-accent); }
  .dbm-crud-input-row input[type="text"]:disabled { opacity:.45; }
  .dbm-crud-null {
    display:inline-flex; align-items:center; gap:4px;
    font-size:11.5px; color:var(--dg-muted); cursor:pointer; white-space:nowrap; flex-shrink:0;
  }
  .dbm-btn-danger { color:#e5484d; border-color:rgba(229,72,77,.35); }
  .dbm-btn-danger:hover:not(:disabled) {
    background:rgba(229,72,77,.08); color:#e5484d; border-color:#e5484d;
  }

  /* ── 增强功能样式 ── */
  .dbm-selected-info { font-size:12px; color:var(--dg-text2); white-space:nowrap; }
  .dbm-selected-info b { color:var(--dg-accent); }

  /* 表列表：分组 + 行操作 */
  .dbm-table-sec-hd {
    display:flex; align-items:center; justify-content:space-between;
    padding:8px 8px 2px; font-size:11.5px; font-weight:600; color:var(--dg-muted); letter-spacing:.3px;
  }
  .dbm-table { display:flex; align-items:center; gap:6px; padding:6px 6px 6px 8px; width:100%;
    border:none; background:none; border-radius:8px; cursor:pointer; font-size:13px; color:var(--dg-text2);
    text-align:left; transition:background .15s; font-family:inherit; }
  .dbm-table:hover { background:var(--dg-card-hover); color:var(--dg-text); }
  .dbm-table-active { background:var(--dg-card-active); color:var(--dg-text); font-weight:600; }
  .dbm-table-actions { display:none; align-items:center; gap:2px; flex-shrink:0; }
  .dbm-table:hover .dbm-table-actions { display:inline-flex; }
  .dbm-table-act { border:none; background:none; cursor:pointer; font-size:12px; color:var(--dg-muted); padding:2px 4px; border-radius:6px; }
  .dbm-table-act:hover { background:var(--dg-card-hover); color:var(--dg-accent); }

  /* 表操作菜单弹层 */

  /* 多选 */
  .dbm-sel-th { width:36px; text-align:center; }
  .dbm-sel-cell { text-align:center; cursor:pointer; }
  .dbm-row-selected { background:color-mix(in srgb, var(--dg-accent) 12%, transparent); box-shadow: inset 2px 0 0 var(--dg-accent), inset 0 0 20px color-mix(in srgb, var(--dg-accent) 6%, transparent); }

  /* 表详情：DDL / 索引 / 触发器 / 外键 */
  .dbm-ddl {
    font-family:ui-monospace,Consolas,"Courier New",monospace; font-size:12px; line-height:1.6;
    background:var(--dg-bg-muted); border:1px solid var(--dg-border-light); border-radius:10px;
    padding:12px; overflow:auto; word-break:break-all; color:var(--dg-text2); white-space:pre-wrap; max-height:45vh;
  }
  .dbm-details { margin:6px 0; border:1px solid var(--dg-border-light); border-radius:8px; overflow:hidden; }
  .dbm-details summary { cursor:pointer; padding:8px 12px; font-size:12px; font-weight:600; color:var(--dg-text2); background:var(--dg-card-active); }
  .dbm-details .dbm-ddl { border:none; border-radius:0; max-height:30vh; }
  .dbm-fk-chips { display:flex; flex-wrap:wrap; gap:6px; padding:8px 0; }
  .dbm-tag-fk { background:color-mix(in srgb, var(--dg-accent) 12%, transparent); color:var(--dg-accent); border:1px solid color-mix(in srgb, var(--dg-accent) 30%, transparent); }

  /* SQL 控制台 */
  .dbm-sql { display:flex; flex-direction:column; gap:8px; height:100%; min-height:0; }
  .dbm-sql-bar { display:flex; align-items:center; gap:8px; flex-shrink:0; }
  .dbm-sql-src { font-size:12px; color:var(--dg-text2); background:var(--dg-bg-muted); padding:3px 10px; border-radius:999px; font-family:ui-monospace,Consolas,monospace; }
  .dbm-sql-meta { font-size:12px; color:var(--dg-muted); }
  .dbm-sql-grid { max-height:52vh; }

  /* 状态概览增强 */
  .dbm-status-grid { display:grid; grid-template-columns:repeat(auto-fit,minmax(360px,1fr)); gap:12px; margin-top:0; align-items:start; }
  .dbm-status-card {
    background:var(--dg-card); border:1px solid var(--dg-border-light); border-radius:12px;
    padding:12px 14px; min-width:0;
  }
  .dbm-status-kv { display:grid; grid-template-columns:110px 1fr; gap:6px 12px; font-size:12px; padding:8px 0; }
  .dbm-status-kv span { color:var(--dg-muted); font-weight:600; }
  .dbm-status-kv code { color:var(--dg-text); word-break:break-all; font-family:ui-monospace,Consolas,monospace; }
  .dbm-status-actions { display:flex; flex-wrap:wrap; gap:8px; padding:6px 0 2px; }
  .dbm-status-ok { font-size:12px; color:#16a34a; padding:6px 0 0; }
  .dbm-status-hint { font-size:12px; color:#d97706; background:color-mix(in srgb, #d97706 10%, transparent); border-radius:8px; padding:8px 10px; margin-top:8px; }
  .dbm-events { display:flex; flex-direction:column; max-height:240px; overflow-y:auto; gap:2px; padding:6px 0; }
  .dbm-event { display:flex; align-items:center; gap:10px; font-size:12px; padding:4px 6px; border-radius:6px; }
  .dbm-event:hover { background:var(--dg-card-hover); }
  .dbm-event-time { color:var(--dg-muted); font-family:ui-monospace,Consolas,monospace; flex-shrink:0; }
  .dbm-event-ty { color:var(--dg-accent); font-weight:600; flex-shrink:0; }
  .dbm-event-title { color:var(--dg-text2); overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .dbm-diag-hex { font-size:11.5px; word-break:break-all; }
  .dbm-integrity { font-size:12px; color:var(--dg-text2); padding:6px 0; }
  .dbm-integrity-hd { font-weight:600; color:var(--dg-muted); margin-bottom:4px; }
  .dbm-integrity-ok { color:#16a34a; font-weight:600; }

  .dbm-stat-col { border:1px solid var(--dg-border-light); border-radius:10px; padding:10px 12px; }
  .dbm-stat-col-hd { display:flex; align-items:center; gap:8px; margin-bottom:6px; font-size:13px; }
  .dbm-stat-row { font-size:12px; color:var(--dg-text2); padding:1px 0; }
  .dbm-stat-tops { display:flex; flex-wrap:wrap; gap:6px; margin-top:6px; }

  /* 表结构对比 */
  .dbm-cmp-grid { display:grid; grid-template-columns:1fr 1fr; gap:12px; }
  .dbm-cmp-side { display:flex; flex-direction:column; gap:6px; }
  .dbm-cmp-label { font-size:12px; font-weight:600; color:var(--dg-text2); }
  .dbm-cmp-chips { display:flex; flex-wrap:wrap; gap:6px; padding:8px 0; }

  /* ── 统一弹窗系统 ── */
  .dbm-modal-stack { display:flex; flex-direction:column; gap:12px; }

  /* ── 右侧抽屉（行检查器） ── */
  @keyframes dbmDrawerIn { from { transform:translateX(100%); opacity:.4; } to { transform:none; opacity:1; } }

  /* 行检查器字段 */
  .dbm-insp { display:flex; flex-direction:column; gap:10px; }
  .dbm-insp-field { border:1px solid var(--dg-border-light); border-radius:10px; overflow:hidden; }
  .dbm-insp-field-hd {
    display:flex; align-items:center; gap:8px; padding:7px 10px; background:var(--dg-card-active);
    font-size:12px; font-weight:600; color:var(--dg-text2); font-family:ui-monospace,Consolas,monospace;
  }
  .dbm-insp-field-name { min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .dbm-insp-ts { font-size:11.5px; color:var(--dg-muted); font-family:ui-monospace,Consolas,monospace; }
  .dbm-insp-field-val { padding:8px 10px; font-size:13px; color:var(--dg-text); word-break:break-all; white-space:pre-wrap; }
  .dbm-insp-actions { margin-left:auto; display:flex; gap:2px; }

  /* 页签胶囊（表结构子导航） */
  .dbm-pills { display:flex; flex-wrap:wrap; gap:6px; }
  .dbm-pill {
    padding:5px 12px; border:1px solid var(--dg-border); border-radius:999px; background:var(--dg-card);
    color:var(--dg-text2); font-size:12px; cursor:pointer; font-family:inherit; transition:all .15s;
  }
  .dbm-pill:hover { border-color:var(--dg-accent); color:var(--dg-text); }
  .dbm-pill-active { background:var(--dg-accent); border-color:var(--dg-accent); color:#fff; font-weight:600; }
  .dbm-pill .dbm-pill-count { opacity:.7; margin-left:4px; }

  /* 分页增强 */
  .dbm-pager-jump { display:inline-flex; align-items:center; gap:4px; font-size:12px; color:var(--dg-muted); }
  .dbm-pager-jump input {
    width:52px; padding:4px 6px; border:1px solid var(--dg-border); border-radius:6px;
    background:var(--dg-card); color:var(--dg-text); font-size:12px; text-align:center; outline:none; font-family:inherit;
  }
  .dbm-pager-jump input:focus { border-color:var(--dg-accent); }

  /* 统计弹窗两列 */
  .dbm-stats-grid { display:grid; grid-template-columns:repeat(auto-fill,minmax(300px,1fr)); gap:10px; }

  /* 常用数据库快捷入口 */
  .dbm-appdbs {
    display: flex; align-items: center; gap: 6px; flex-wrap: wrap;
    padding: 7px 14px; border-bottom: 1px solid var(--dg-border-light);
    background: color-mix(in srgb, var(--dg-bg-muted) 96%, transparent);
  }
  .dbm-appdbs-label { font-size: 11.5px; color: var(--dg-muted); margin-right: 2px; }
  .dbm-appdb {
    display: inline-flex; align-items: center; gap: 4px;
    padding: 3px 10px; font-size: 11.5px; border-radius: 999px;
    border: 1px solid var(--dg-border); background: var(--dg-card); color: var(--dg-text2); cursor: pointer;
    transition: all .13s ease; white-space: nowrap;
  }
  .dbm-appdb:hover { border-color: var(--dg-accent); color: var(--dg-text); }
  .dbm-appdb-on { background: color-mix(in srgb, var(--dg-accent) 18%, transparent); border-color: var(--dg-accent); color: var(--dg-text); font-weight: 600; }
</style>
