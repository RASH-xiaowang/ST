// 数据库管理 — Tauri IPC 封装层
// 组件层统一通过本模块调用后端，避免直接 invoke。
import { invoke } from '@tauri-apps/api/core';
import type {
  DbAppDatabase,
  DbBackupResult,
  DbCellValue,
  DbCleanupResult,
  DbColumn,
  DbEvent,
  DbExportResult,
  DbFileEntry,
  DbHeaderInfo,
  DbInfo,
  DbIntegrityResult,
  DbRestoreResult,
  DbSqlResult,
  DbTableData,
  DbTableDetail,
  DbTableStats,
} from '../types';

/** 表数据查询参数（keyset 分页 + 过滤/排序；与后端 TableQueryParams 对应） */
export type DbTableQuery = {
  table: string;
  page: number;
  pageSize: number;
  orderCol: string;
  orderDir: string;
  filter: string;
  recount: boolean;
  cursor: string | null;
  direction: string;
};

/** 单元格读取参数（dbPath 供外部库） */
export type DbCellQuery = {
  dbPath?: string | null;
  table: string;
  rowid: number;
  column: string;
};

export const dbApi = {
  // ── 表 / 结构 ──
  listTables: () => invoke<string[]>('list_tables'),
  externalListTables: (dbPath: string | null) => invoke<string[]>('external_list_tables', { dbPath }),
  tableSchema: (table: string) => invoke<DbColumn[]>('table_schema', { table }),
  externalTableSchema: (dbPath: string | null, table: string) =>
    invoke<DbColumn[]>('external_table_schema', { dbPath, table }),
  getTableDetail: (dbPath: string | null, table: string) =>
    invoke<DbTableDetail>('get_table_detail', { dbPath, table }),
  tableStats: (dbPath: string | null, table: string, sample: number) =>
    invoke<DbTableStats>('table_stats', { dbPath, table, sample }),

  // ── 数据 ──
  queryTable: (args: DbTableQuery) => invoke<DbTableData>('query_table', args),
  externalQueryTable: (dbPath: string | null, args: DbTableQuery) =>
    invoke<DbTableData>('external_query_table', { dbPath, ...args }),
  getCellValue: (args: DbCellQuery) => invoke<DbCellValue>('get_cell_value', args),
  insertRow: (table: string, data: Record<string, unknown>) =>
    invoke<number>('insert_row', { table, data }),
  updateRow: (table: string, rowid: number, data: Record<string, unknown>) =>
    invoke<void>('update_row', { table, rowid, data }),
  deleteRow: (table: string, rowid: number) => invoke<void>('delete_row', { table, rowid }),
  runSql: (dbPath: string | null, sql: string, limit: number) =>
    invoke<DbSqlResult>('run_sql', { dbPath, sql, limit }),

  // ── 导出 / 备份 / 运维 ──
  exportTableCsv: (dbPath: string | null, table: string, path: string) =>
    invoke<DbExportResult>('export_table_csv', { dbPath, table, path }),
  getDbInfo: () => invoke<DbInfo>('get_db_info'),
  listAppDatabases: () => invoke<DbAppDatabase[]>('list_app_databases'),
  getAppDataDirs: () =>
    invoke<{ appBase?: string; dataDir?: string; wechatDataDir?: string; logsDir?: string; scanDirs?: string[] }>(
      'get_app_data_dirs'
    ),
  checkDbHeader: (dbPath: string | null) => invoke<DbHeaderInfo>('check_db_header', { dbPath }),
  scanExternalDbs: (dir: string) => invoke<DbFileEntry[]>('scan_external_dbs', { dir }),
  dbIntegrity: (dbPath: string | null) => invoke<DbIntegrityResult>('db_integrity', { dbPath }),
  backupInternalDb: (path: string) => invoke<DbBackupResult>('backup_internal_db', { path }),
  restoreInternalDb: (backupPath: string) =>
    invoke<DbRestoreResult>('restore_internal_db', { backupPath }),
  cleanupOldData: () => invoke<DbCleanupResult>('cleanup_old_data'),
  queryEvents: (limit: number, offset: number) => invoke<DbEvent[]>('query_events', { limit, offset }),

  // ── 配置 / 文件 ──
  getDbConfig: () => invoke<{ key: string; value: string }[]>('get_db_config'),
  setDbConfig: (key: string, value: string) => invoke<void>('set_db_config', { key, value }),
  writeFile: (path: string, contentB64: string) =>
    invoke<void>('write_file', { path, contentB64 }),
};
