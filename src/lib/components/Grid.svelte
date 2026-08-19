<script lang="ts">
	import type { Cell, Sort } from '$lib/api';
	import { cn } from '$lib/utils';
	import * as ContextMenu from '$lib/components/ui/context-menu';
	import { writeText } from '@tauri-apps/plugin-clipboard-manager';
	import { toast } from 'svelte-sonner';

	let {
		columns,
		rows,
		sort = null,
		editable = false,
		pkIndex = -1,
		onsort,
		oneditcell,
		ondeleterow
	}: {
		columns: string[];
		rows: Cell[][];
		sort?: Sort | null;
		editable?: boolean;
		pkIndex?: number;
		onsort?: (column: string) => void;
		oneditcell?: (rowIdx: number, colIdx: number, value: string | null) => void;
		ondeleterow?: (rowIdx: number) => void;
	} = $props();

	let editing: { row: number; col: number } | null = $state(null);
	let editValue = $state('');
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

	function startEdit(r: number, c: number) {
		if (!editable) return;
		editing = { row: r, col: c };
		const v = rows[r][c];
		editValue = v === null ? '' : typeof v === 'object' ? JSON.stringify(v) : String(v);
	}

	function commit(asNull = false) {
		if (!editing) return;
		oneditcell?.(editing.row, editing.col, asNull ? null : editValue);
		editing = null;
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
								editable && c !== pkIndex && 'cursor-text'
							)}
							ondblclick={() => c !== pkIndex && startEdit(r, c)}
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
			{#if editable}
				<ContextMenu.Separator />
				<ContextMenu.Item
					disabled={menuAt.col === pkIndex}
					onclick={() => startEdit(menuAt.row, menuAt.col)}
				>
					Edit cell
				</ContextMenu.Item>
				<ContextMenu.Item
					disabled={menuAt.col === pkIndex}
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
