<script lang="ts">
	import type { Cell, Sort } from '$lib/api';
	import { cn } from '$lib/utils';
	import * as ContextMenu from '$lib/components/ui/context-menu';
	import * as Dialog from '$lib/components/ui/dialog';
	import { Button } from '$lib/components/ui/button';
	import { Textarea } from '$lib/components/ui/textarea';
	import JsonEditor from '$lib/components/JsonEditor.svelte';
	import { writeText } from '@tauri-apps/plugin-clipboard-manager';
	import { toast } from 'svelte-sonner';

	let {
		columns,
		rows,
		sort = null,
		editable = false,
		pkIndexes = [],
		fks = {},
		onsort,
		oneditcell,
		ondeleterow,
		onfollowfk
	}: {
		columns: string[];
		rows: Cell[][];
		sort?: Sort | null;
		editable?: boolean;
		/// primary key column indexes — those cells stay read-only
		pkIndexes?: number[];
		/// column index → the table/column that column points at
		fks?: Record<number, { ref_table: string; ref_column: string }>;
		onsort?: (column: string) => void;
		oneditcell?: (rowIdx: number, colIdx: number, value: string | null) => void;
		ondeleterow?: (rowIdx: number) => void;
		onfollowfk?: (refTable: string, refColumn: string, value: Cell) => void;
	} = $props();

	let editing: { row: number; col: number } | null = $state(null);
	/// same cell coordinates, but shown in a dialog because the value is too
	/// big for a one-line input; `canEdit` is false for pk cells and read-only tables
	let expanded: { row: number; col: number; canEdit: boolean; json: boolean } | null =
		$state(null);
	let editValue = $state('');
	/// past this many characters an inline input is useless
	const BIG = 120;
	// which cell the context menu was opened on
	let menuAt: { row: number; col: number } = $state({ row: 0, col: 0 });

	async function copy(text: string, what: string) {
		await writeText(text);
		toast.success(`${what} copied`);
	}

	/** The row as an object, so it pastes into a JSON body or a test fixture. */
	function rowAsJson(r: number) {
		return JSON.stringify(
			Object.fromEntries(columns.map((c, i) => [c, rows[r][i]])),
			null,
			2
		);
	}

	function display(v: Cell): string {
		if (v === null) return 'NULL';
		if (typeof v === 'object') return JSON.stringify(v);
		return String(v);
	}

	/// the editable text of a cell; JSON is indented for the dialog, where
	/// there's room to read it
	function cellText(v: Cell, pretty = false): string {
		if (v === null) return '';
		if (typeof v === 'object') return JSON.stringify(v, null, pretty ? 2 : 0);
		return String(v);
	}

	/// only JSON gets the syntax-highlighted editor; a long plain string in a
	/// text column is just text
	function isJson(v: Cell): boolean {
		if (typeof v === 'object' && v !== null) return true;
		const t = cellText(v).trim();
		if (!(t.startsWith('{') || t.startsWith('['))) return false;
		try {
			JSON.parse(t);
			return true;
		} catch {
			return false;
		}
	}

	function isBig(v: Cell): boolean {
		if (typeof v === 'object' && v !== null) return true;
		const t = cellText(v);
		return t.length > BIG || t.includes('\n');
	}

	/// double-click, or the context menu. Big values open in a dialog — and do
	/// so even when the cell isn't editable, since seeing the whole thing is
	/// half the point.
	function startEdit(r: number, c: number) {
		const v = rows[r][c];
		const canEdit = editable && !pkIndexes.includes(c);
		if (isBig(v)) {
			// re-indent JSON that arrived as a string too, not just decoded objects
			const asJson = isJson(v);
			editValue = asJson ? JSON.stringify(JSON.parse(cellText(v)), null, 2) : cellText(v);
			expanded = { row: r, col: c, canEdit, json: asJson };
			return;
		}
		if (!canEdit) return;
		editValue = cellText(v);
		editing = { row: r, col: c };
	}

	function commit() {
		const at = editing ?? expanded;
		if (!at) return;
		const value = editValue;
		editing = null;
		expanded = null; // close first: the caller may open a confirm dialog
		oneditcell?.(at.row, at.col, value);
	}
</script>

<ContextMenu.Root>
	<ContextMenu.Trigger class="block h-full">
		<div class="h-full overflow-auto rounded-lg border">
	<table class="w-full border-collapse font-mono text-xs">
		<thead class="sticky top-0 z-10 bg-card">
			<tr>
				{#if editable && ondeleterow}<th class="w-8 border-b"></th>{/if}
				{#each columns as col (col)}
					<th
						class="cursor-pointer border-b px-3 py-2 text-left font-semibold whitespace-nowrap text-primary select-none hover:bg-muted"
						onclick={() => onsort?.(col)}
					>
						{col}
						{#if sort?.column === col}<span class="text-foreground">{sort.desc ? '↓' : '↑'}</span>{/if}
					</th>
				{/each}
			</tr>
		</thead>
		<tbody>
			{#each rows as row, r (r)}
				<tr class="hover:bg-muted/40">
					{#if editable && ondeleterow}
						<td class="border-b px-1 text-center">
							<button
								class="text-muted-foreground hover:text-destructive"
								title="Delete row"
								onclick={() => ondeleterow?.(r)}>×</button
							>
						</td>
					{/if}
					{#each row as cell, c (c)}
						<td
							class={cn(
								'max-w-96 truncate border-b px-3 py-1.5 whitespace-nowrap',
								cell === null && 'text-muted-foreground italic',
								editable && !pkIndexes.includes(c) && 'cursor-text'
							)}
							ondblclick={() => startEdit(r, c)}
						oncontextmenu={() => (menuAt = { row: r, col: c })}
						>
							{#if editing && editing.row === r && editing.col === c}
								<!-- svelte-ignore a11y_autofocus -->
								<input
									class="w-full min-w-32 border border-primary bg-background px-1 outline-none"
									autofocus
									bind:value={editValue}
									onkeydown={(e) => {
										if (e.key === 'Enter') commit();
										else if (e.key === 'Escape') editing = null;
									}}
									onblur={() => (editing = null)}
								/>
							{:else}
								{display(cell)}
								{#if fks[c] && cell !== null}
									<button
										class="ml-1 text-primary hover:underline"
										title="Go to {fks[c].ref_table}.{fks[c].ref_column} = {display(cell)}"
										onclick={() => onfollowfk?.(fks[c].ref_table, fks[c].ref_column, cell)}
										>↗</button
									>
								{/if}
							{/if}
						</td>
					{/each}
				</tr>
			{:else}
				<tr><td class="px-3 py-4 text-muted-foreground" colspan={columns.length + 1}>no rows</td></tr>
			{/each}
		</tbody>
	</table>
		</div>
	</ContextMenu.Trigger>
	<ContextMenu.Content class="w-60">
		{#if rows.length > 0}
			<ContextMenu.Item
				onclick={() => copy(display(rows[menuAt.row][menuAt.col]), 'Cell value')}
			>
				Copy cell
			</ContextMenu.Item>
			<ContextMenu.Item onclick={() => copy(rowAsJson(menuAt.row), 'Row JSON')}>
				Copy row as JSON
			</ContextMenu.Item>
			<ContextMenu.Item onclick={() => copy(columns[menuAt.col], 'Column name')}>
				Copy column name
			</ContextMenu.Item>
			<ContextMenu.Item onclick={() => startEdit(menuAt.row, menuAt.col)}>
				{editable && !pkIndexes.includes(menuAt.col) ? 'Expand / edit' : 'Expand'}
			</ContextMenu.Item>
			{#if fks[menuAt.col] && rows[menuAt.row][menuAt.col] !== null}
				<ContextMenu.Item
					onclick={() =>
						onfollowfk?.(
							fks[menuAt.col].ref_table,
							fks[menuAt.col].ref_column,
							rows[menuAt.row][menuAt.col]
						)}
				>
					Go to {fks[menuAt.col].ref_table}
				</ContextMenu.Item>
			{/if}
			{#if editable}
				<ContextMenu.Separator />
				<ContextMenu.Item
					disabled={pkIndexes.includes(menuAt.col)}
					onclick={() => oneditcell?.(menuAt.row, menuAt.col, null)}
				>
					Set NULL
				</ContextMenu.Item>
				{#if ondeleterow}
					<ContextMenu.Separator />
					<ContextMenu.Item variant="destructive" onclick={() => ondeleterow?.(menuAt.row)}>
						Delete row
					</ContextMenu.Item>
				{/if}
			{/if}
		{:else}
			<ContextMenu.Item disabled>no rows</ContextMenu.Item>
		{/if}
	</ContextMenu.Content>
</ContextMenu.Root>

<Dialog.Root open={!!expanded} onOpenChange={(o: boolean) => !o && (expanded = null)}>
	<Dialog.Content class="sm:max-w-3xl">
		<Dialog.Header>
			<Dialog.Title class="font-mono text-sm">
				{expanded ? columns[expanded.col] : ''}
			</Dialog.Title>
		</Dialog.Header>
		<div class="h-[60vh]">
			{#if expanded?.json}
				{#key expanded.row + ':' + expanded.col}
					<JsonEditor bind:value={editValue} readonly={!expanded.canEdit} onsave={commit} />
				{/key}
			{:else}
				<Textarea
					bind:value={editValue}
					readonly={!expanded?.canEdit}
					spellcheck="false"
					class="h-full resize-none font-mono text-xs"
					onkeydown={(e: KeyboardEvent) => {
						// plain Enter is a newline here, so saving needs the modifier
						if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) commit();
					}}
				/>
			{/if}
		</div>
		<Dialog.Footer>
			{#if expanded?.canEdit}
				<Button size="sm" variant="outline" onclick={() => (expanded = null)}>Cancel</Button>
				<Button size="sm" onclick={commit}>Save</Button>
			{:else}
				<Button size="sm" variant="outline" onclick={() => (expanded = null)}>Close</Button>
			{/if}
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
