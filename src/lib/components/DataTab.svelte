<script lang="ts">
	import {
		api,
		type Cell,
		type ColumnInfo,
		type Filter,
		type ForeignKey,
		type QueryResult,
		type Sort
	} from '$lib/api';
	import Grid from '$lib/components/Grid.svelte';
	import Spinner from '$lib/components/Spinner.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import * as Select from '$lib/components/ui/select';
	import * as Dialog from '$lib/components/ui/dialog';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu';
	import { toast } from 'svelte-sonner';
	import { save as saveDialog } from '@tauri-apps/plugin-dialog';
	import { confirm } from '$lib/confirm.svelte';

	let {
		connId,
		table,
		filter = null,
		onfollowfk
	}: {
		connId: string;
		table: string;
		/// set when we arrived here by following a foreign key
		filter?: Filter | null;
		onfollowfk?: (refTable: string, refColumn: string, value: Cell) => void;
	} = $props();

	const OPS = [
		'eq',
		'neq',
		'lt',
		'lte',
		'gt',
		'gte',
		'contains',
		'startswith',
		'endswith',
		'like',
		'notlike',
		'ilike',
		'notilike',
		'in',
		'notin',
		'null',
		'notnull'
	] as const;
	const OP_LABEL: Record<string, string> = {
		eq: '=',
		neq: '≠',
		lt: '<',
		lte: '≤',
		gt: '>',
		gte: '≥',
		contains: 'contains',
		startswith: 'starts with',
		endswith: 'ends with',
		like: 'LIKE',
		notlike: 'NOT LIKE',
		ilike: 'ILIKE',
		notilike: 'NOT ILIKE',
		in: 'IN',
		notin: 'NOT IN',
		null: 'IS NULL',
		notnull: 'IS NOT NULL'
	};
	// hint shown in the value box — these ops expect a specific shape
	const OP_HINT: Record<string, string> = {
		like: '%pattern%',
		notlike: '%pattern%',
		ilike: '%pattern%',
		notilike: '%pattern%',
		in: 'a, b, c',
		notin: 'a, b, c'
	};
	const LIMITS = [50, 100, 200, 500, 1000];

	let limit = $state(200);
	let loading = $state(false);
	let schema: ColumnInfo[] = $state([]);
	let result: QueryResult | null = $state(null);
	let filters: Filter[] = $state([]);
	let sort: Sort | null = $state(null);
	let page = $state(0);
	let total: number | null = $state(null);
	let fks: ForeignKey[] = $state([]);
	let insertOpen = $state(false);
	let insertValues: Record<string, string> = $state({});

	const pkColumns = $derived(schema.filter((c) => c.is_pk).map((c) => c.name));
	const pkIndexes = $derived.by(() => {
		const cols = result?.columns ?? [];
		return pkColumns.map((n) => cols.indexOf(n));
	});
	// a key column missing from the result set means we can't name the row
	const editable = $derived(pkColumns.length > 0 && pkIndexes.every((i) => i >= 0));

	/// every primary key column of one row, keyed by column name
	function pkValues(rowIdx: number): Record<string, Cell> {
		const row = result!.rows[rowIdx];
		return Object.fromEntries(pkColumns.map((n, i) => [n, row[pkIndexes[i]]]));
	}
	const fkByIndex = $derived.by(() => {
		const cols = result?.columns ?? [];
		const out: Record<number, ForeignKey> = {};
		for (const fk of fks) {
			const i = cols.indexOf(fk.column);
			if (i >= 0) out[i] = fk;
		}
		return out;
	});

	$effect(() => {
		table; // react to table switch, and to arriving via a foreign key
		filters = filter ? [{ ...filter }] : [];
		sort = null;
		page = 0;
		schema = [];
		result = null; // drop the previous table's grid instead of flashing stale rows
		load();
	});

	async function load() {
		loading = true;
		// read `filters` only after an await: a synchronous read here would make
		// the $effect below depend on the state it writes, and loop
		let snapshot: Filter[] = [];
		try {
			schema = await api.tableSchema(connId, table);
			fks = await api.foreignKeys(connId, table).catch(() => []);
			snapshot = $state.snapshot(filters) as Filter[];
			const r = await api.fetchRows(connId, table, snapshot, sort, limit, page * limit);
			// empty result set loses column names — fall back to schema
			if (r.columns.length === 0) r.columns = schema.map((c) => c.name);
			result = r;
		} catch (e) {
			toast.error(String(e));
		} finally {
			loading = false;
		}
		// count(*) can be slow on a big table, so the grid never waits for it
		total = null;
		const counting = table;
		try {
			const n = await api.countRows(connId, table, snapshot);
			if (counting === table) total = n;
		} catch {
			// a failed count just means no "of N" — the rows are already on screen
		}
	}

	function applySort(column: string) {
		sort = sort?.column === column && !sort.desc ? { column, desc: true } : { column, desc: false };
		page = 0;
		load();
	}

	function coerce(dt: string, raw: string | null): Cell {
		if (raw === null) return null;
		const t = dt.toLowerCase();
		if (/int|serial/.test(t) && /^-?\d+$/.test(raw)) return Number(raw);
		if (/real|double|float|numeric|decimal/.test(t) && !isNaN(Number(raw))) return Number(raw);
		if (/bool/.test(t) && (raw === 'true' || raw === 'false')) return raw === 'true';
		return raw;
	}

	function show(v: Cell): string {
		if (v === null) return 'NULL';
		if (typeof v === 'object') return JSON.stringify(v);
		return String(v);
	}

	/// "id = 7", or "a = 1 and b = 'x'" for a composite key
	function rowLabel(rowIdx: number): string {
		return Object.entries(pkValues(rowIdx))
			.map(([col, v]) => `${col} = ${show(v)}`)
			.join(' and ');
	}

	/// JSON reformatted in the expand dialog isn't a change; compare the parsed
	/// shape before asking the user to confirm one
	function same(a: Cell, b: Cell): boolean {
		if (show(a) === show(b)) return true;
		try {
			return JSON.stringify(JSON.parse(show(a))) === JSON.stringify(JSON.parse(show(b)));
		} catch {
			return false;
		}
	}

	/// long values (a jsonb blob) would push the confirm dialog off-screen
	const clip = (v: Cell) => (show(v).length > 300 ? show(v).slice(0, 300) + '…' : show(v));

	async function editCell(rowIdx: number, colIdx: number, value: string | null) {
		if (!result || !editable) return;
		const col = result.columns[colIdx];
		const dt = schema.find((c) => c.name === col)?.data_type ?? 'text';
		const next = coerce(dt, value);
		const prev = result.rows[rowIdx][colIdx];
		if (same(prev, next)) return;
		const keys = pkValues(rowIdx);
		const ok = await confirm(
			`${col}: ${clip(prev)} → ${clip(next)}\n\nRow where ${rowLabel(rowIdx)}`,
			{ title: `Update ${table}`, okLabel: 'Update' }
		);
		if (!ok) return;
		try {
			await api.updateCell(connId, table, col, next, keys);
			load();
		} catch (e) {
			toast.error(String(e));
		}
	}

	async function deleteRow(rowIdx: number) {
		if (!result || !editable) return;
		const keys = pkValues(rowIdx);
		const ok = await confirm(`Delete the row where ${rowLabel(rowIdx)}?`, {
			title: `Delete row from ${table}`
		});
		if (!ok) return;
		try {
			await api.deleteRow(connId, table, keys);
			load();
		} catch (e) {
			toast.error(String(e));
		}
	}

	async function insert() {
		const values: Record<string, Cell> = {};
		for (const c of schema) {
			const raw = insertValues[c.name];
			if (raw !== undefined && raw !== '') values[c.name] = coerce(c.data_type, raw);
		}
		try {
			await api.insertRow(connId, table, values);
			insertOpen = false;
			insertValues = {};
			load();
		} catch (e) {
			toast.error(String(e));
		}
	}

	async function exportTable(format: 'csv' | 'json') {
		const path = await saveDialog({
			defaultPath: `${table}.${format}`,
			filters: [{ name: format.toUpperCase(), extensions: [format] }]
		});
		if (!path) return;
		try {
			// same filters and sort as the grid, without the page limit
			const n = await api.exportTable(
				connId,
				table,
				$state.snapshot(filters),
				sort,
				format,
				path
			);
			toast.success(`exported ${n} rows`);
		} catch (e) {
			toast.error(String(e));
		}
	}

	function addFilter() {
		filters.push({ column: schema[0]?.name ?? '', op: 'eq', value: '' });
	}
</script>

<div class="flex h-full flex-col gap-3">
	<div class="flex flex-wrap items-center gap-2">
		{#each filters as f, i (i)}
			<div class="flex items-center gap-1 rounded-lg border bg-card p-1">
				<Select.Root type="single" bind:value={f.column}>
					<Select.Trigger size="sm" class="h-7 font-mono text-xs">{f.column}</Select.Trigger>
					<Select.Content>
						{#each schema as c (c.name)}<Select.Item value={c.name} label={c.name} />{/each}
					</Select.Content>
				</Select.Root>
				<Select.Root type="single" bind:value={f.op}>
					<Select.Trigger size="sm" class="h-7 w-28 font-mono text-xs">{OP_LABEL[f.op]}</Select.Trigger>
					<Select.Content>
						{#each OPS as op (op)}<Select.Item value={op} label={OP_LABEL[op]} />{/each}
					</Select.Content>
				</Select.Root>
				{#if f.op !== 'null' && f.op !== 'notnull'}
					<Input
						class="h-7 w-36 font-mono text-xs"
						placeholder={OP_HINT[f.op] ?? 'value'}
						bind:value={f.value}
						onkeydown={(e: KeyboardEvent) => e.key === 'Enter' && (page = 0, load())}
					/>
				{/if}
				<button
					class="px-1 text-muted-foreground hover:text-destructive"
					onclick={() => {
						filters.splice(i, 1);
						page = 0;
						load();
					}}>×</button
				>
			</div>
		{/each}
		<Button size="sm" variant="outline" onclick={addFilter}>+ filter</Button>
		{#if filters.length > 0}
			<Button size="sm" onclick={() => ((page = 0), load())}>Apply</Button>
		{/if}
		<div class="grow"></div>
		{#if loading && result}
			<Spinner class="mr-1" />
		{/if}
		<Select.Root
			type="single"
			value={String(limit)}
			onValueChange={(v: string) => {
				limit = Number(v);
				page = 0;
				load();
			}}
		>
			<Select.Trigger size="sm" class="h-8 w-24 font-mono text-xs">{limit} rows</Select.Trigger>
			<Select.Content>
				{#each LIMITS as n (n)}<Select.Item value={String(n)} label="{n} rows" />{/each}
			</Select.Content>
		</Select.Root>
		{#if editable}
			<Button size="sm" variant="outline" onclick={() => (insertOpen = true)}>+ row</Button>
		{/if}
		<DropdownMenu.Root>
			<DropdownMenu.Trigger>
				{#snippet child({ props })}
					<Button {...props} size="sm" variant="outline">Export</Button>
				{/snippet}
			</DropdownMenu.Trigger>
			<DropdownMenu.Content>
				<DropdownMenu.Item onclick={() => exportTable('csv')}>CSV</DropdownMenu.Item>
				<DropdownMenu.Item onclick={() => exportTable('json')}>JSON</DropdownMenu.Item>
			</DropdownMenu.Content>
		</DropdownMenu.Root>
	</div>

	{#if !editable && schema.length > 0}
		<p class="text-xs text-muted-foreground">
			read-only: table needs a primary key for editing
		</p>
	{/if}

	{#if !result && loading}
		<div class="flex min-h-0 flex-1 items-center justify-center">
			<Spinner class="size-6" />
		</div>
	{:else if result}
		<div class="min-h-0 flex-1">
			<Grid
				columns={result.columns}
				rows={result.rows}
				{sort}
				{editable}
				{pkIndexes}
				fks={fkByIndex}
				onsort={applySort}
				{onfollowfk}
				oneditcell={editCell}
				ondeleterow={editable ? deleteRow : undefined}
			/>
		</div>
		<div class="flex items-center gap-3 text-xs text-muted-foreground">
			<Button size="sm" variant="ghost" disabled={page === 0} onclick={() => (page--, load())}
				>← prev</Button
			>
			<span>
				<!-- built as strings: svelte trims the leading space inside an {#if} -->
				page {page + 1}{total === null ? '' : ` of ${Math.max(1, Math.ceil(total / limit))}`} ·
				{result.rows.length} rows{total === null ? '' : ` of ${total}`}
			</span>
			<Button
				size="sm"
				variant="ghost"
				disabled={total !== null ? (page + 1) * limit >= total : result.rows.length < limit}
				onclick={() => (page++, load())}>next →</Button
			>
		</div>
	{/if}
</div>

<Dialog.Root bind:open={insertOpen}>
	<Dialog.Content class="max-h-[80vh] overflow-y-auto sm:max-w-md">
		<Dialog.Header><Dialog.Title>Insert row into {table}</Dialog.Title></Dialog.Header>
		<div class="grid gap-2">
			{#each schema as c (c.name)}
				<label class="grid grid-cols-3 items-center gap-2 text-xs">
					<span class="truncate font-mono" title={c.data_type}>
						{c.name}{#if c.is_pk}<span class="text-primary"> pk</span>{/if}
					</span>
					<Input
						class="col-span-2 h-8 font-mono text-xs"
						placeholder={c.data_type + (c.nullable ? '' : ' · required')}
						bind:value={insertValues[c.name]}
					/>
				</label>
			{/each}
		</div>
		<Dialog.Footer>
			<Button onclick={insert}>Insert</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
