<script lang="ts">
	import type { Cell, Sort } from '$lib/api';
	import { cn } from '$lib/utils';

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
