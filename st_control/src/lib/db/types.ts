/* ============================================================
 * 数据库管理 — 共享类型
 * 表格浏览/CRUD 的数据结构（行字段动态，用 Record 精确表达）。
 * ============================================================ */

/** 表列元信息 */
export interface DbColumn {
  name: string;
  col_type?: string;
  not_null?: boolean;
  pk?: boolean;
  default?: string | null;
  [key: string]: unknown;
}

/** 外部数据库文件条目（scan_external_dbs 返回，对应 Rust `DbFileInfo`） */
export interface DbFileEntry {
  path: string;
  name: string;
  size_bytes: number;
  mtime_ms?: number;
  [key: string]: unknown;
}

/** 表数据行（字段名动态，rowid 由后端附带） */
export type DbRow = Record<string, unknown> & { rowid?: string | number };

/** 表格数据分页结果 */
export interface DbTableData {
  columns: DbColumn[];
  rows: DbRow[];
  total: number;
  page: number;
  page_size: number;
  next_cursor: string | null;
  prev_cursor: string | null;
}

/** 数据库事件（query_events 返回条目） */
export interface DbEvent {
  timestamp?: string;
  event_type?: string;
  title?: string;
  [key: string]: unknown;
}

/** 单元格原始值（get_cell_value 返回，对应 Rust `cell_value_to_json`） */
export type DbCellValue =
  | { kind: 'null' }
  | { kind: 'text'; text: string }
  | {
      kind: 'blob';
      length: number;
      base64: string;
      mime: string;
      is_image: boolean;
      hex_preview: string;
    }
  | { kind: 'error'; text: string };

/** 应用自管理数据库条目（list_app_databases 返回） */
export interface DbAppDatabase {
  key: string;
  label: string;
  name: string;
  path: string;
  size_bytes: number;
}

/** 内置数据库信息（get_db_info 返回，对应 Rust `DbInfo`） */
export interface DbInfo {
  path: string;
  size_bytes: number;
  event_count: number;
  task_count: number;
  agent_log_count: number;
}

/** 索引信息（get_table_detail 返回项） */
export interface DbIndexInfo {
  seq: number;
  name: string;
  unique: boolean;
  origin: string;
  partial: boolean;
  columns?: string[];
}

/** 触发器信息（get_table_detail 返回项） */
export interface DbTriggerInfo {
  name: string;
  sql: string;
}

/** 表详情（get_table_detail 返回，对应 Rust `table_detail`） */
export interface DbTableDetail {
  table: string;
  ddl: string;
  indexes: DbIndexInfo[];
  triggers: DbTriggerInfo[];
  foreign_keys: string[];
}

/** 列统计（table_stats 返回项，对应 Rust `ColStat` JSON） */
export interface DbColumnStat {
  name: string;
  type: string;
  sample: number;
  non_null: number;
  null_count: number;
  null_pct: number;
  is_numeric: boolean;
  min: number | null;
  max: number | null;
  sum: number | null;
  top: { value: string; count: number }[];
}

/** 表统计（table_stats 返回） */
export interface DbTableStats {
  table: string;
  sample: number;
  columns: DbColumnStat[];
}

/** 完整性检查结果（db_integrity 返回） */
export interface DbIntegrityResult {
  integrity: string[];
  foreign_keys: { table: string; rowid: number; parent: string; fkid: number }[];
}

/** 任意 SQL 执行结果（run_sql 返回，查询/写入两种形态） */
export type DbSqlResult =
  | { kind: 'query'; columns: string[]; rows: DbRow[]; truncated: boolean }
  | { kind: 'write'; affected: number };

/** CSV 导出结果（export_table_csv 返回） */
export interface DbExportResult {
  path: string;
  count: number;
}

/** 内置库备份结果（backup_internal_db 返回） */
export interface DbBackupResult {
  path: string;
  size_bytes: number;
}

/** 内置库恢复准备结果（restore_internal_db 返回） */
export interface DbRestoreResult {
  restore_path: string;
  hint: string;
}

/** 文件头诊断（check_db_header 返回） */
export interface DbHeaderInfo {
  path: string;
  size_bytes: number;
  is_sqlite: boolean;
  header_hex: string;
  header_text: string;
  page_count: number;
}

/** 旧数据清理结果（cleanup_old_data 返回，对应 Rust `CleanupResult`） */
export interface DbCleanupResult {
  deleted_events: number;
  deleted_agent: number;
  days: number;
}
