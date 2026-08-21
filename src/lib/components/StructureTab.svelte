<script lang="ts">
	import { api, type ColumnInfo, type Engine, type TableRef } from '$lib/api';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import * as Command from '$lib/components/ui/command';
	import * as Popover from '$lib/components/ui/popover';
	import CheckIcon from '@lucide/svelte/icons/check';
	import ChevronsUpDownIcon from '@lucide/svelte/icons/chevrons-up-down';
	import { toast } from 'svelte-sonner';
	import { untrack } from 'svelte';
	import { cn } from '$lib/utils';
	import { ENGINES } from '$lib/dialect';

	let {
		connId,
		table,
		engine,
		onchanged
	}: { connId: string; table: TableRef; engine: Engine; onchanged?: () => void } = $props();

	const typeGroups = $derived(ENGINES[engine].types);
	const knownTypes = $derived(typeGroups.flatMap(([, types]) => types));

	let schema: ColumnInfo[] = $state([]);
	let name = $state('');
	// engine is fixed for a mounted tab (one connection per workspace), so read it once
	let colType = $state(ENGINES[untrack(() => engine)].defaultType);
	let nullable = $state(true);
	let defaultValue = $state('');
	let typeOpen = $state(false);
	let typeSearch = $state('');

	// anything the user types that isn't in the list is offered verbatim —
	// covers sizes we don't preset, e.g. numeric(12,4) or text[]
	const customType = $derived.by(() => {
		const t = typeSearch.trim();
		return t && !knownTypes.includes(t) ? t : null;
	});

	function pickType(t: string) {
		colType = t;
		typeSearch = '';
		typeOpen = false;
	}

	$effect(() => {
		// both parts: same-named tables in two schemas are different tables
		table.schema;
		table.name;
		load();
	});

	async function load() {
		try {
			schema = await api.tableSchema(connId, table);
		} catch (e) {
			toast.error(String(e));
		}
	}

	async function addColumn() {
		try {
			await api.addColumn(connId, table, name, colType, nullable, defaultValue || null);
			toast.success(`column ${name} added`);
			name = '';
			defaultValue = '';
			load();
			onchanged?.();
		} catch (e) {
			toast.error(String(e));
		}
	}
</script>

<div class="flex max-w-2xl flex-col gap-6">
	<table class="w-full font-mono text-xs">
		<thead>
			<tr class="text-left text-primary">
				<th class="border-b px-3 py-2">column</th>
				<th class="border-b px-3 py-2">type</th>
				<th class="border-b px-3 py-2">nullable</th>
				<th class="border-b px-3 py-2">pk</th>
			</tr>
		</thead>
		<tbody>
			{#each schema as c (c.name)}
				<tr class="hover:bg-muted/40">
					<td class="border-b px-3 py-1.5">{c.name}</td>
					<td class="border-b px-3 py-1.5 text-muted-foreground">{c.data_type}</td>
					<td class="border-b px-3 py-1.5">{c.nullable ? 'yes' : 'no'}</td>
					<td class="border-b px-3 py-1.5">{c.is_pk ? '✓' : ''}</td>
				</tr>
			{/each}
		</tbody>
	</table>

	<div class="rounded-xl border bg-card p-4">
		<h3 class="mb-3 text-sm font-semibold">Add column</h3>
		<div class="flex flex-wrap items-center gap-2">
			<Input class="w-40 font-mono text-xs" placeholder="name" bind:value={name} />
			<Popover.Root bind:open={typeOpen}>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button
							{...props}
							variant="outline"
							size="sm"
							role="combobox"
							aria-expanded={typeOpen}
							class="w-44 justify-between font-mono text-xs"
						>
							<span class="truncate">{colType}</span>
							<ChevronsUpDownIcon class="size-3.5 shrink-0 opacity-50" />
						</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content class="w-56 p-0">
					<Command.Root>
						<Command.Input placeholder="Search or type…" bind:value={typeSearch} />
						<Command.List>
							<Command.Empty class="py-4 text-xs">no type found</Command.Empty>
							{#if customType}
								<Command.Group heading="Custom">
									<Command.Item
										value={customType}
										onSelect={() => pickType(customType)}
										class="font-mono text-xs"
									>
										<CheckIcon class="size-3.5 opacity-0" />
										use "{customType}"
									</Command.Item>
								</Command.Group>
							{/if}
							{#each typeGroups as [label, types] (label)}
								<Command.Group heading={label}>
									{#each types as t (t)}
										<Command.Item value={t} onSelect={() => pickType(t)} class="font-mono text-xs">
											<CheckIcon class={cn('size-3.5', colType !== t && 'opacity-0')} />
											{t}
										</Command.Item>
									{/each}
								</Command.Group>
							{/each}
						</Command.List>
					</Command.Root>
				</Popover.Content>
			</Popover.Root>
			<Input
				class="w-36 font-mono text-xs"
				placeholder="default (optional)"
				bind:value={defaultValue}
			/>
			<label class="flex items-center gap-1.5 text-xs">
				<input type="checkbox" bind:checked={nullable} class="accent-primary" /> nullable
			</label>
			<Button size="sm" disabled={!name || !colType} onclick={addColumn}>Add</Button>
		</div>
	</div>
</div>
