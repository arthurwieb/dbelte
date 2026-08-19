<script lang="ts">
	import { api, type Cell, type ColumnInfo, type Filter, type QueryResult, type Sort } from '$lib/api';
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

	let { connId, table }: { connId: string; table: string } = $props();

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
	let insertOpen = $state(false);
	let insertValues: Record<string, string> = $state({});

	const pk = $derived(schema.find((c) => c.is_pk));
	const pkIndex = $derived.by(() => {
		const r = result;
		const p = pk;
		return r && p ? r.columns.indexOf(p.name) : -1;
	});
	const editable = $derived(schema.filter((c) => c.is_pk).length === 1);

	$effect(() => {
		table; // react to table switch
		filters = [];
		sort = null;
		page = 0;
		schema = [];
		result = null; // drop the previous table's grid instead of flashing stale rows
		load();
	});

	async function load() {
		loading = true;
		try {
			schema = await api.tableSchema(connId, table);
			const r = await api.fetchRows(
				connId,
				table,
				$state.snapshot(filters),
				sort,
				limit,
				page * limit
			);
			// empty result set loses column names — fall back to schema
			if (r.columns.length === 0) r.columns = schema.map((c) => c.name);
			result = r;
		} catch (e) {
			toast.error(String(e));
		} finally {
			loading = false;
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

	async function editCell(rowIdx: number, colIdx: number, value: string | null) {
		if (!result || pkIndex < 0) return;
		const col = result.columns[colIdx];
		const dt = schema.find((c) => c.name === col)?.data_type ?? 'text';
		try {
			await api.updateCell(connId, table, col, coerce(dt, value), result.rows[rowIdx][pkIndex]);
			load();
		} catch (e) {
			toast.error(String(e));
		}
	}

	async function deleteRow(rowIdx: number) {
		if (!result || pkIndex < 0) return;
		if (!(await confirm('Delete this row?', { title: `Delete row from ${table}` }))) return;
		try {
			await api.deleteRow(connId, table, result.rows[rowIdx][pkIndex]);
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
			const quoted = `"${table.replaceAll('"', '""')}"`;
			const n = await api.exportRows(connId, `SELECT * FROM ${quoted}`, format, path);
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
			read-only: table needs exactly one primary key column for editing
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
				{pkIndex}
				onsort={applySort}
				oneditcell={editCell}
				ondeleterow={editable ? deleteRow : undefined}
			/>
		</div>
		<div class="flex items-center gap-3 text-xs text-muted-foreground">
			<Button size="sm" variant="ghost" disabled={page === 0} onclick={() => (page--, load())}
				>← prev</Button
			>
			<span>page {page + 1} · {result.rows.length} rows</span>
			<Button
				size="sm"
				variant="ghost"
				disabled={result.rows.length < limit}
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
