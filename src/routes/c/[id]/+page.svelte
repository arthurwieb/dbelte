<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { api, type Connection, type SavedQuery } from '$lib/api';
	import DataTab from '$lib/components/DataTab.svelte';
	import StructureTab from '$lib/components/StructureTab.svelte';
	import QueryTab from '$lib/components/QueryTab.svelte';
	import Spinner from '$lib/components/Spinner.svelte';
	import * as Tabs from '$lib/components/ui/tabs';
	import * as ContextMenu from '$lib/components/ui/context-menu';
	import { writeText } from '@tauri-apps/plugin-clipboard-manager';
	import { Button } from '$lib/components/ui/button';
	import { toast } from 'svelte-sonner';
	import { confirm } from '$lib/confirm.svelte';

	const connId = page.params.id!;

	let conn: Connection | null = $state(null);
	let tables: string[] = $state([]);
	let savedQueries: SavedQuery[] = $state([]);
	let selectedTable: string | null = $state(null);
	let activeTab = $state('data');
	let sql = $state('');
	let cmSchema: Record<string, string[]> = $state({});
	let loading = $state(true);

	async function init() {
		try {
			const all = await api.listConnections();
			conn = all.find((c) => c.id === connId) ?? null;
			if (!conn) throw new Error('connection not found');
			tables = await api.listTables(connId);
			selectedTable = tables[0] ?? null;
			refreshQueries();
			buildCmSchema();
		} catch (e) {
			toast.error(String(e));
		} finally {
			loading = false;
		}
	}
	init();

	async function refreshQueries() {
		savedQueries = await api.listSavedQueries(connId);
	}

	// table/column map for SQL autocomplete; refreshed after DDL
	async function buildCmSchema() {
		const map: Record<string, string[]> = {};
		for (const t of tables) {
			try {
				map[t] = (await api.tableSchema(connId, t)).map((c) => c.name);
			} catch {
				// table may have vanished; skip
			}
		}
		cmSchema = map;
	}

	async function refreshTables() {
		tables = await api.listTables(connId);
		buildCmSchema();
	}

	const quoted = (t: string) => `"${t.replaceAll('"', '""')}"`;

	/** Drop a starter statement into the Query tab, ready to run or edit. */
	function seedQuery(text: string) {
		sql = text;
		activeTab = 'query';
	}

	function openQuery(q: SavedQuery) {
		sql = q.sql;
		activeTab = 'query';
	}

	async function removeQuery(q: SavedQuery) {
		if (!(await confirm(`"${q.name}" will be removed.`, { title: 'Delete saved query' }))) return;
		await api.deleteSavedQuery(q.id);
		refreshQueries();
	}

	// --- sidebar resize ---
	const MIN_W = 160;
	const MAX_W = 560;
	// ssr is off, so localStorage is safe at init
	let sidebarWidth = $state(Number(localStorage.getItem('sidebarWidth')) || 240);

	function clampWidth(w: number) {
		return Math.min(MAX_W, Math.max(MIN_W, w));
	}

	function persistWidth() {
		localStorage.setItem('sidebarWidth', String(sidebarWidth));
	}

	function startResize(e: PointerEvent) {
		e.preventDefault();
		const startX = e.clientX;
		const startW = sidebarWidth;
		// kill text selection while dragging over the grid
		document.body.style.userSelect = 'none';
		const move = (ev: PointerEvent) => (sidebarWidth = clampWidth(startW + ev.clientX - startX));
		const up = () => {
			window.removeEventListener('pointermove', move);
			window.removeEventListener('pointerup', up);
			document.body.style.userSelect = '';
			persistWidth();
		};
		window.addEventListener('pointermove', move);
		window.addEventListener('pointerup', up);
	}

	function resizeKeys(e: KeyboardEvent) {
		const step = e.shiftKey ? 40 : 10;
		if (e.key === 'ArrowLeft') sidebarWidth = clampWidth(sidebarWidth - step);
		else if (e.key === 'ArrowRight') sidebarWidth = clampWidth(sidebarWidth + step);
		else return;
		e.preventDefault();
		persistWidth();
	}

	async function leave() {
		await api.disconnect(connId);
		goto('/');
	}
</script>

<div class="flex h-full">
	<aside
		class="relative flex shrink-0 flex-col border-r bg-card"
		style="width: {sidebarWidth}px"
	>
		<div class="flex items-center gap-2 border-b p-3">
			<button class="text-muted-foreground hover:text-primary" title="Back" onclick={leave}
				>←</button
			>
			<div class="min-w-0">
				<div class="truncate text-sm font-semibold">{conn?.name ?? '…'}</div>
				<div class="truncate font-mono text-xs text-muted-foreground">{conn?.database}</div>
			</div>
		</div>

		<div class="min-h-0 flex-1 overflow-y-auto p-2">
			<div class="mb-1 px-1">
				<span class="text-xs font-semibold tracking-wide text-primary uppercase">Saved queries</span>
			</div>
			{#each savedQueries as q (q.id)}
				<div class="group flex items-center">
					<button
						class="min-w-0 grow truncate rounded-md px-2 py-1 text-left text-xs hover:bg-muted"
						title={q.sql}
						onclick={() => openQuery(q)}>{q.name}</button
					>
					<button
						class="px-1 text-muted-foreground opacity-0 group-hover:opacity-100 hover:text-destructive"
						onclick={() => removeQuery(q)}>×</button
					>
				</div>
			{:else}
				<p class="px-2 text-xs text-muted-foreground">none yet</p>
			{/each}

			<div class="mt-4 mb-1 flex items-center justify-between px-1">
				<span class="text-xs font-semibold tracking-wide text-primary uppercase">Tables</span>
				<button
					class="text-xs text-muted-foreground hover:text-primary"
					title="Refresh"
					onclick={refreshTables}>⟳</button
				>
			</div>
			{#if loading}
				<div class="flex justify-center py-6"><Spinner /></div>
			{/if}
			{#each tables as t (t)}
				<ContextMenu.Root>
					<ContextMenu.Trigger class="block w-full">
						<button
							class="block w-full truncate rounded-md px-2 py-1 text-left font-mono text-xs hover:bg-muted {selectedTable ===
							t
								? 'bg-primary/15 text-primary'
								: ''}"
							onclick={() => {
								selectedTable = t;
								if (activeTab === 'query') activeTab = 'data';
							}}>{t}</button
						>
					</ContextMenu.Trigger>
					<ContextMenu.Content class="w-56">
						<ContextMenu.Item onclick={() => seedQuery(`SELECT * FROM ${quoted(t)} LIMIT 100;`)}>
							SELECT * in query tab
						</ContextMenu.Item>
						<ContextMenu.Item
							onclick={() => seedQuery(`SELECT count(*) FROM ${quoted(t)};`)}
						>
							Count rows
						</ContextMenu.Item>
						<ContextMenu.Separator />
						<ContextMenu.Item
							onclick={() => {
								selectedTable = t;
								activeTab = 'structure';
							}}
						>
							View structure
						</ContextMenu.Item>
						<ContextMenu.Item onclick={() => writeText(t)}>Copy table name</ContextMenu.Item>
					</ContextMenu.Content>
				</ContextMenu.Root>
			{:else}
				{#if !loading}<p class="px-2 text-xs text-muted-foreground">no tables</p>{/if}
			{/each}
		</div>

		<!-- a focusable separator is the ARIA window-splitter pattern; the lint rule
		     doesn't model it, hence the suppressions -->
		<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
		<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
		<div
			class="absolute inset-y-0 -right-1 z-20 w-2 cursor-col-resize transition-colors hover:bg-primary/50 focus-visible:bg-primary focus-visible:outline-none active:bg-primary"
			role="separator"
			aria-orientation="vertical"
			aria-label="Resize sidebar"
			aria-valuenow={sidebarWidth}
			aria-valuemin={MIN_W}
			aria-valuemax={MAX_W}
			tabindex="0"
			ondblclick={() => ((sidebarWidth = 240), persistWidth())}
			onpointerdown={startResize}
			onkeydown={resizeKeys}
		></div>
	</aside>

	<main class="flex min-w-0 flex-1 flex-col p-4">
		<Tabs.Root bind:value={activeTab} class="flex min-h-0 flex-1 flex-col">
			<div class="mb-3 flex items-center gap-3">
				<Tabs.List>
					<Tabs.Trigger value="data" disabled={!selectedTable}>Data</Tabs.Trigger>
					<Tabs.Trigger value="structure" disabled={!selectedTable}>Structure</Tabs.Trigger>
					<Tabs.Trigger value="query">Query</Tabs.Trigger>
				</Tabs.List>
				{#if selectedTable && activeTab !== 'query'}
					<span class="font-mono text-sm text-muted-foreground">{selectedTable}</span>
				{/if}
			</div>
			<Tabs.Content value="data" class="min-h-0 flex-1">
				{#if selectedTable}
					<DataTab {connId} table={selectedTable} />
				{/if}
			</Tabs.Content>
			<Tabs.Content value="structure" class="min-h-0 flex-1 overflow-y-auto">
				{#if selectedTable && conn}
					<StructureTab
						{connId}
						table={selectedTable}
						engine={conn.engine}
						onchanged={buildCmSchema}
					/>
				{/if}
			</Tabs.Content>
			<Tabs.Content value="query" class="min-h-0 flex-1">
				{#if conn}
					<QueryTab
						{connId}
						engine={conn.engine}
						schema={cmSchema}
						bind:sql
						onsaved={refreshQueries}
					/>
				{/if}
			</Tabs.Content>
		</Tabs.Root>
	</main>
</div>
