import { invoke } from '@tauri-apps/api/core';

export type Engine = 'postgres' | 'sqlite';

export interface Connection {
	id: string;
	name: string;
	engine: Engine;
	host: string | null;
	port: number | null;
	database: string;
	username: string | null;
}

export interface SavedQuery {
	id: string;
	connection_id: string;
	name: string;
	sql: string;
}

export interface ColumnInfo {
	name: string;
	data_type: string;
	nullable: boolean;
	is_pk: boolean;
}

export type Cell = string | number | boolean | null | object;

export interface QueryResult {
	columns: string[];
	rows: Cell[][];
	rows_affected: number;
}

export interface Filter {
	column: string;
	op:
		| 'eq'
		| 'neq'
		| 'lt'
		| 'lte'
		| 'gt'
		| 'gte'
		| 'contains'
		| 'startswith'
		| 'endswith'
		| 'like'
		| 'notlike'
		| 'ilike'
		| 'notilike'
		| 'in'
		| 'notin'
		| 'null'
		| 'notnull';
	value: string;
}

export interface Sort {
	column: string;
	desc: boolean;
}

/** Backend's error text for a cancelled query — not worth a toast. */
export const CANCELLED = 'query cancelled';

export const api = {
	listConnections: () => invoke<Connection[]>('list_connections'),
	saveConnection: (conn: Connection, password?: string) =>
		invoke<Connection>('save_connection', { conn, password }),
	deleteConnection: (id: string) => invoke<void>('delete_connection', { id }),
	testConnection: (conn: Connection, password?: string) =>
		invoke<void>('test_connection', { conn, password }),
	connect: (id: string) => invoke<void>('connect', { id }),
	disconnect: (id: string) => invoke<void>('disconnect', { id }),
	listTables: (id: string) => invoke<string[]>('list_tables', { id }),
	tableSchema: (id: string, table: string) =>
		invoke<ColumnInfo[]>('table_schema', { id, table }),
	fetchRows: (
		id: string,
		table: string,
		filters: Filter[],
		sort: Sort | null,
		limit: number,
		offset: number
	) => invoke<QueryResult>('fetch_rows', { id, table, filters, sort, limit, offset }),
	runQuery: (id: string, sql: string, queryId: string) =>
		invoke<QueryResult>('run_query', { id, sql, queryId }),
	cancelQuery: (queryId: string) => invoke<void>('cancel_query', { queryId }),
	updateCell: (id: string, table: string, column: string, value: Cell, pkValue: Cell) =>
		invoke<number>('update_cell', { id, table, column, value, pkValue }),
	insertRow: (id: string, table: string, values: Record<string, Cell>) =>
		invoke<number>('insert_row', { id, table, values }),
	deleteRow: (id: string, table: string, pkValue: Cell) =>
		invoke<number>('delete_row', { id, table, pkValue }),
	addColumn: (
		id: string,
		table: string,
		name: string,
		colType: string,
		nullable: boolean,
		defaultValue: string | null
	) => invoke<void>('add_column', { id, table, name, colType, nullable, defaultValue }),
	listSavedQueries: (connectionId: string) =>
		invoke<SavedQuery[]>('list_saved_queries', { connectionId }),
	saveQuery: (query: SavedQuery) => invoke<SavedQuery>('save_query', { query }),
	deleteSavedQuery: (id: string) => invoke<void>('delete_saved_query', { id }),
	exportRows: (id: string, sql: string, format: 'csv' | 'json', path: string) =>
		invoke<number>('export_rows', { id, sql, format, path })
};
