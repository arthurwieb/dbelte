<script lang="ts">
	import { api, type Engine, type QueryResult } from '$lib/api';
	import Grid from '$lib/components/Grid.svelte';
	import SqlEditor from '$lib/components/SqlEditor.svelte';
	import Spinner from '$lib/components/Spinner.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import * as Dialog from '$lib/components/ui/dialog';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu';
	import { toast } from 'svelte-sonner';
	import { save as saveDialog } from '@tauri-apps/plugin-dialog';

	let {
		connId,
		engine,
		schema = {},
		sql = $bindable(''),
		onsaved
	}: {
		connId: string;
		engine: Engine;
		schema?: Record<string, string[]>;
		sql?: string;
		onsaved?: () => void;
	} = $props();

	let result: QueryResult | null = $state(null);
	let running = $state(false);
	let saveOpen = $state(false);
	let queryName = $state('');

	async function run() {
		if (!sql.trim()) return;
		running = true;
		try {
			result = await api.runQuery(connId, sql);
			if (result.columns.length === 0) {
				toast.success(`${result.rows_affected} rows affected`);
			}
		} catch (e) {
			toast.error(String(e));
		} finally {
			running = false;
		}
	}

	async function saveQuery() {
		try {
			await api.saveQuery({ id: '', connection_id: connId, name: queryName, sql });
			toast.success('query saved');
			saveOpen = false;
			queryName = '';
			onsaved?.();
		} catch (e) {
			toast.error(String(e));
		}
	}

	async function exportResult(format: 'csv' | 'json') {
		const path = await saveDialog({
			defaultPath: `query.${format}`,
			filters: [{ name: format.toUpperCase(), extensions: [format] }]
		});
		if (!path) return;
		try {
			const n = await api.exportRows(connId, sql, format, path);
			toast.success(`exported ${n} rows`);
		} catch (e) {
			toast.error(String(e));
		}
	}
</script>

<div class="flex h-full flex-col gap-3">
	<div class="h-48 shrink-0">
		<SqlEditor bind:value={sql} {engine} {schema} onrun={run} />
	</div>
	<div class="flex items-center gap-2">
		<Button size="sm" disabled={running || !sql.trim()} onclick={run}>
			{#if running}<Spinner class="mr-1 size-3.5 text-current" />{/if}
			Run <span class="ml-1 text-xs opacity-60">⌘⏎</span>
		</Button>
		<Button size="sm" variant="outline" disabled={!sql.trim()} onclick={() => (saveOpen = true)}>
			Save query
		</Button>
		{#if result && result.columns.length > 0}
			<DropdownMenu.Root>
				<DropdownMenu.Trigger>
					{#snippet child({ props })}
						<Button {...props} size="sm" variant="outline">Export</Button>
					{/snippet}
				</DropdownMenu.Trigger>
				<DropdownMenu.Content>
					<DropdownMenu.Item onclick={() => exportResult('csv')}>CSV</DropdownMenu.Item>
					<DropdownMenu.Item onclick={() => exportResult('json')}>JSON</DropdownMenu.Item>
				</DropdownMenu.Content>
			</DropdownMenu.Root>
			<span class="text-xs text-muted-foreground">{result.rows.length} rows</span>
		{/if}
	</div>
	{#if result && result.columns.length > 0}
		<div class="min-h-0 flex-1">
			<Grid columns={result.columns} rows={result.rows} />
		</div>
	{/if}
</div>

<Dialog.Root bind:open={saveOpen}>
	<Dialog.Content class="sm:max-w-sm">
		<Dialog.Header><Dialog.Title>Save query</Dialog.Title></Dialog.Header>
		<Input
			placeholder="Query name"
			bind:value={queryName}
			onkeydown={(e: KeyboardEvent) => e.key === 'Enter' && queryName && saveQuery()}
		/>
		<Dialog.Footer>
			<Button disabled={!queryName} onclick={saveQuery}>Save</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
