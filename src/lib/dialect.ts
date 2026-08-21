import { PostgreSQL, SQLite, MySQL, MSSQL, type SQLDialect } from '@codemirror/lang-sql';
import type { Engine, TableRef } from '$lib/api';

/**
 * Everything the UI needs to know about an engine, in one table. Adding an
 * engine is a row here plus a `DbPool` arm in Rust; the components below read
 * this rather than branching on the engine themselves.
 */
export type EngineSpec = {
	label: string;
	/** false means a local file, so no host/port/username */
	server: boolean;
	defaultPort?: number;
	/** CodeMirror's SQL dialect: keywords, types and built-in identifiers */
	cm: SQLDialect;
	/** sql-formatter's language key */
	formatter: 'postgresql' | 'sqlite' | 'mysql' | 'tsql';
	/**
	 * The schema the engine implies when none is written. `tableLabel` strips it
	 * from the completion keys, so lang-sql has to be told what it was or
	 * `public.orders` completes to nothing.
	 */
	defaultSchema?: string;
	/**
	 * Where a blank Database field lands you. SQL Server takes the login's
	 * default when none is named, so leaving it empty is legitimate there —
	 * every other engine needs one, and `undefined` is what makes the form
	 * insist on it.
	 */
	blankDatabase?: string;
	/** mirrors `Dialect::quote_ident` in src-tauri/src/db.rs */
	quote: (name: string) => string;
	/**
	 * A starter `SELECT` capped at `n` rows. SQL Server has no `LIMIT`; its
	 * `OFFSET…FETCH` needs an `ORDER BY`, so a seed query uses `TOP` instead.
	 */
	preview: (quotedTable: string, n: number) => string;
	/** grouped for the add-column dropdown; parameterised ones come pre-sized */
	types: [string, string[]][];
	/** the type pre-selected in that dropdown */
	defaultType: string;
};

const dq = (name: string) => `"${name.replaceAll('"', '""')}"`;
const limitOffset = (t: string, n: number) => `SELECT * FROM ${t} LIMIT ${n};`;

export const ENGINES: Record<Engine, EngineSpec> = {
	postgres: {
		label: 'PostgreSQL',
		server: true,
		defaultPort: 5432,
		cm: PostgreSQL,
		formatter: 'postgresql',
		defaultSchema: 'public',
		quote: dq,
		preview: limitOffset,
		defaultType: 'text',
		types: [
			['Text', ['text', 'varchar(255)', 'char(1)', 'uuid']],
			[
				'Numeric',
				[
					'integer',
					'bigint',
					'smallint',
					'serial',
					'bigserial',
					'numeric(10,2)',
					'real',
					'double precision'
				]
			],
			['Date & time', ['date', 'timestamptz', 'timestamp', 'time', 'timetz', 'interval']],
			['Other', ['boolean', 'jsonb', 'json', 'bytea', 'inet', 'cidr', 'macaddr', 'money', 'xml']],
			['Arrays', ['text[]', 'integer[]', 'uuid[]', 'jsonb[]']]
		]
	},
	sqlite: {
		label: 'SQLite',
		server: false,
		cm: SQLite,
		formatter: 'sqlite',
		quote: dq,
		preview: limitOffset,
		defaultType: 'TEXT',
		// SQLite only has 5 storage classes; the rest are affinities people expect to type
		types: [
			['Storage classes', ['TEXT', 'INTEGER', 'REAL', 'BLOB', 'NUMERIC']],
			['Affinities', ['BOOLEAN', 'DATE', 'DATETIME', 'VARCHAR(255)']]
		]
	},
	mysql: {
		label: 'MySQL / MariaDB',
		server: true,
		defaultPort: 3306,
		cm: MySQL,
		formatter: 'mysql',
		quote: (name) => `\`${name.replaceAll('`', '``')}\``,
		preview: limitOffset,
		defaultType: 'text',
		types: [
			['Text', ['text', 'varchar(255)', 'char(1)', 'longtext', 'tinytext']],
			[
				'Numeric',
				['int', 'bigint', 'smallint', 'tinyint', 'decimal(10,2)', 'float', 'double']
			],
			['Date & time', ['date', 'datetime', 'timestamp', 'time', 'year']],
			['Other', ['boolean', 'json', 'blob', 'longblob']]
		]
	},
	mssql: {
		label: 'SQL Server',
		server: true,
		defaultPort: 1433,
		cm: MSSQL,
		formatter: 'tsql',
		defaultSchema: 'dbo',
		blankDatabase: 'master',
		quote: (name) => `[${name.replaceAll(']', ']]')}]`,
		preview: (t, n) => `SELECT TOP ${n} * FROM ${t};`,
		defaultType: 'nvarchar(255)',
		types: [
			['Text', ['nvarchar(255)', 'nvarchar(max)', 'varchar(255)', 'nchar(1)', 'char(1)']],
			[
				'Numeric',
				['int', 'bigint', 'smallint', 'tinyint', 'decimal(10,2)', 'money', 'float', 'real']
			],
			['Date & time', ['date', 'datetime2', 'datetimeoffset', 'time', 'smalldatetime']],
			['Other', ['bit', 'uniqueidentifier', 'varbinary(max)', 'xml']]
		]
	}
};

/** `"reporting"."orders"` — for SQL we hand to the Query tab. */
export function quoteTable(engine: Engine, t: TableRef): string {
	const q = ENGINES[engine].quote;
	return t.schema ? `${q(t.schema)}.${q(t.name)}` : q(t.name);
}

/**
 * What the user sees: the schema is noise until it isn't the default one.
 * Doubles as the key in the editor's completion map, which is why `SqlEditor`
 * has to hand lang-sql the same `defaultSchema` this strips.
 */
export function tableLabel(engine: Engine, t: TableRef): string {
	return t.schema && t.schema !== ENGINES[engine].defaultSchema
		? `${t.schema}.${t.name}`
		: t.name;
}
