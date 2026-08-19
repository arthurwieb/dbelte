<script lang="ts">
	import { api, CANCELLED, type Engine, type QueryResult } from '$lib/api';
	import Grid from '$lib/components/Grid.svelte';
	import SqlEditor from '$lib/components/SqlEditor.svelte';
	import * as ContextMenu from '$lib/components/ui/context-menu';
	import { writeText } from '@tauri-apps/plugin-clipboard-manager';
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
	let cancelling = $state(false);
	let queryId = '';
	let editor: {
		format: () => void;
		selectAll: () => void;
		clear: () => void;
		focus: () => void;
	} | undefined = $state();
	let saveOpen = $state(false);
	let queryName = $state('');

	async function run() {
		if (!sql.trim() || running) return;
		queryId = crypto.randomUUID();
		running = true;
		try {
			const r = await api.runQuery(connId, sql, queryId);
			result = r;
			if (r.columns.length === 0) {
				toast.success(`${r.rows_affected} rows affected`);
			}
		} catch (e) {
			// cancelling is a deliberate act, not an error worth shouting about
			if (String(e).includes(CANCELLED)) toast.info('query cancelled');
			else toast.error(String(e));
		} finally {
			running = false;
			cancelling = false;
		}
	}

	async function cancel() {
		cancelling = true;
		try {
			await api.cancelQuery(queryId);
		} catch (e) {
			toast.error(String(e));
			cancelling = false;
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
	<ContextMenu.Root>
		<ContextMenu.Trigger class="h-48 shrink-0">
			<SqlEditor bind:value={sql} bind:api={editor} {engine} {schema} onrun={run} />
		</ContextMenu.Trigger>
		<ContextMenu.Content class="w-56">
			<ContextMenu.Item disabled={!sql.trim() || running} onclick={run}>
				Run
				<ContextMenu.Shortcut>⌘⏎</ContextMenu.Shortcut>
			</ContextMenu.Item>
			<ContextMenu.Item disabled={!sql.trim()} onclick={() => editor?.format()}>
				Format SQL
				<ContextMenu.Shortcut>⇧⌥F</ContextMenu.Shortcut>
			</ContextMenu.Item>
			<ContextMenu.Separator />
			<ContextMenu.Item disabled={!sql.trim()} onclick={() => editor?.selectAll()}>
				Select all
			</ContextMenu.Item>
			<ContextMenu.Item disabled={!sql.trim()} onclick={() => writeText(sql)}>
				Copy all
			</ContextMenu.Item>
			<ContextMenu.Item disabled={!sql.trim()} onclick={() => (saveOpen = true)}>
				Save query…
			</ContextMenu.Item>
			<ContextMenu.Separator />
			<ContextMenu.Item
				disabled={!sql.trim()}
				variant="destructive"
				onclick={() => editor?.clear()}
			>
				Clear editor
			</ContextMenu.Item>
		</ContextMenu.Content>
	</ContextMenu.Root>
	<div class="flex items-center gap-2">
		{#if running}
			<Button size="sm" variant="destructive" disabled={cancelling} onclick={cancel}>
				<Spinner class="mr-1 size-3.5 text-current" />
				{cancelling ? 'Cancelling…' : 'Cancel'}
			</Button>
		{:else}
			<Button size="sm" disabled={!sql.trim()} onclick={run}>
				Run <span class="ml-1 text-xs opacity-60">⌘⏎</span>
			</Button>
		{/if}
		<Button size="sm" variant="outline" disabled={!sql.trim()} onclick={() => editor?.format()}>
			Format
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
