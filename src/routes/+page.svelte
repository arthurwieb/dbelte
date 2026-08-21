<script lang="ts">
	import { goto } from '$app/navigation';
	import { api, type Connection, type Engine } from '$lib/api';
	import { ENGINES } from '$lib/dialect';
	import { Button } from '$lib/components/ui/button';
	import Spinner from '$lib/components/Spinner.svelte';
	import { Input } from '$lib/components/ui/input';
	import * as Dialog from '$lib/components/ui/dialog';
	import * as Select from '$lib/components/ui/select';
	import { toast } from 'svelte-sonner';
	import { confirm } from '$lib/confirm.svelte';
	import { open as openDialog } from '@tauri-apps/plugin-dialog';
	import FolderOpenIcon from '@lucide/svelte/icons/folder-open';
	import HeartIcon from '@lucide/svelte/icons/heart';
	import { openUrl } from '@tauri-apps/plugin-opener';
	import { KOFI_URL } from '$lib/links';

	let connections: Connection[] = $state([]);
	let open = $state(false);
	let busy = $state(false);
	let connectingId: string | null = $state(null);

	const blank = (): Connection & { password: string } => ({
		id: '',
		name: '',
		engine: 'postgres' as Engine,
		host: 'localhost',
		port: 5432,
		database: '',
		username: '',
		password: ''
	});
	let form = $state(blank());
	let pasteUrl = $state('');

	/**
	 * Switching engine follows its default port, unless the user typed one of
	 * their own. Checked against every engine's default, not just the one being
	 * left, so hopping through SQLite — which hides the field — doesn't strand
	 * the previous engine's port on the next one.
	 */
	const DEFAULT_PORTS = Object.values(ENGINES).map((e) => e.defaultPort);
	function pickEngine(engine: Engine) {
		const untouched = !form.port || DEFAULT_PORTS.includes(form.port);
		form.engine = engine;
		if (untouched) form.port = ENGINES[engine].defaultPort ?? form.port;
	}

	/** URL schemes people actually paste, mapped to the engine they mean */
	const URL_SCHEMES: [RegExp, Engine][] = [
		[/^postgres(ql)?:\/\//i, 'postgres'],
		[/^mysql:\/\//i, 'mysql'],
		[/^mariadb:\/\//i, 'mysql'],
		[/^(mssql|sqlserver):\/\//i, 'mssql']
	];

	// fill form from a pasted connection string (postgres://…, mysql://…, or a file path)
	function applyUrl(raw: string) {
		const s = raw.trim();
		if (!s) return;
		try {
			const scheme = URL_SCHEMES.find(([re]) => re.test(s));
			if (scheme) {
				const [re, engine] = scheme;
				const u = new URL(s.replace(re, 'http://'));
				form.engine = engine;
				form.host = u.hostname || form.host;
				form.port = u.port ? Number(u.port) : (ENGINES[engine].defaultPort ?? form.port);
				form.database = decodeURIComponent(u.pathname.replace(/^\//, '')) || form.database;
				form.username = decodeURIComponent(u.username) || form.username;
				if (u.password) form.password = decodeURIComponent(u.password);
				if (!form.name) form.name = form.database || u.hostname;
			} else if (/^sqlite:/i.test(s) || /\.(db|sqlite3?)$/i.test(s)) {
				form.engine = 'sqlite';
				form.database = s.replace(/^sqlite:(\/\/)?/i, '');
				if (!form.name) form.name = form.database.split('/').pop() ?? '';
			} else {
				return;
			}
			pasteUrl = '';
			toast.success('fields filled from URL');
		} catch {
			toast.error('could not parse connection string');
		}
	}

	// the webview must not navigate away — hand the URL to the OS browser
	const supportUs = () => openUrl(KOFI_URL);

	async function browseSqlite() {
		const path = await openDialog({
			multiple: false,
			directory: false,
			filters: [
				{ name: 'SQLite database', extensions: ['db', 'sqlite', 'sqlite3', 'db3'] },
				{ name: 'All files', extensions: ['*'] }
			]
		});
		if (typeof path !== 'string') return; // cancelled
		form.database = path;
		if (!form.name) form.name = path.split(/[/\\]/).pop() ?? '';
	}

	async function refresh() {
		try {
			connections = await api.listConnections();
		} catch (e) {
			toast.error(String(e));
		}
	}
	refresh();

	function edit(c: Connection) {
		form = { ...c, password: '' };
		open = true;
	}

	function toConn(): Connection {
		const { password: _pw, ...conn } = form;
		return { ...conn, port: conn.port ? Number(conn.port) : null };
	}

	async function save() {
		busy = true;
		try {
			await api.saveConnection(toConn(), form.password || undefined);
			toast.success('saved');
			open = false;
			await refresh();
		} catch (e) {
			toast.error(String(e));
		} finally {
			busy = false;
		}
	}

	async function test() {
		busy = true;
		try {
			await api.testConnection(toConn(), form.password || undefined);
			toast.success('connection ok');
		} catch (e) {
			toast.error(String(e));
		} finally {
			busy = false;
		}
	}

	async function remove(c: Connection) {
		if (
			!(await confirm(`"${c.name}" and its saved password will be removed.`, {
				title: 'Delete connection'
			}))
		)
			return;
		try {
			await api.deleteConnection(c.id);
			await refresh();
		} catch (e) {
			toast.error(String(e));
		}
	}

	async function enter(c: Connection) {
		if (connectingId) return;
		connectingId = c.id;
		try {
			await api.connect(c.id);
			await goto(`/c/${c.id}`);
		} catch (e) {
			toast.error(String(e));
		} finally {
			connectingId = null;
		}
	}
</script>

<div class="mx-auto max-w-4xl p-8">
	<header class="mb-8 flex items-center justify-between">
		<h1 class="text-2xl font-bold"><span class="text-primary">db</span>elte</h1>
		<div class="flex items-center gap-2">
			<Button variant="ghost" size="sm" title="Support dbelte on Ko-fi" onclick={supportUs}>
				<HeartIcon class="size-4 text-primary" /> Support
			</Button>
			<Button
			onclick={() => {
				form = blank();
				open = true;
			}}>New connection</Button
			>
		</div>
	</header>

	{#if connections.length === 0}
		<p class="mt-24 text-center text-muted-foreground">
			No connections yet — create one to get started.
		</p>
	{/if}

	<div class="grid grid-cols-1 gap-4 sm:grid-cols-2">
		{#each connections as c (c.id)}
			<div
				class="group cursor-pointer rounded-xl border bg-card p-4 transition-colors hover:border-primary {connectingId ===
				c.id
					? 'pointer-events-none border-primary opacity-70'
					: ''}"
				role="button"
				tabindex="0"
				onclick={() => enter(c)}
				onkeydown={(e) => e.key === 'Enter' && enter(c)}
			>
				<div class="flex items-center justify-between">
					<span class="flex items-center gap-2 font-semibold">
						{c.name}
						{#if connectingId === c.id}<Spinner />{/if}
					</span>
					<span
						class="rounded-md bg-primary/15 px-2 py-0.5 font-mono text-xs text-primary uppercase"
						>{c.engine}</span
					>
				</div>
				<p class="mt-1 truncate font-mono text-xs text-muted-foreground">
					{ENGINES[c.engine].server
						? `${c.username}@${c.host}:${c.port}${c.database ? `/${c.database}` : ''}`
						: c.database}
				</p>
				<div class="mt-3 flex gap-2 opacity-0 transition-opacity group-hover:opacity-100">
					<Button
						size="sm"
						variant="outline"
						onclick={(e: MouseEvent) => {
							e.stopPropagation();
							edit(c);
						}}>Edit</Button
					>
					<Button
						size="sm"
						variant="destructive"
						onclick={(e: MouseEvent) => {
							e.stopPropagation();
							remove(c);
						}}>Delete</Button
					>
				</div>
			</div>
		{/each}
	</div>
</div>

<Dialog.Root bind:open>
	<Dialog.Content class="sm:max-w-md">
		<Dialog.Header>
			<Dialog.Title>{form.id ? 'Edit connection' : 'New connection'}</Dialog.Title>
		</Dialog.Header>
		<div class="grid gap-3">
			<Input
				class="font-mono text-xs"
				placeholder="Paste connection URL to autofill (postgres://…, mysql://…)"
				bind:value={pasteUrl}
				oninput={() => applyUrl(pasteUrl)}
			/>
			<Input placeholder="Name" bind:value={form.name} />
			<Select.Root
				type="single"
				value={form.engine}
				onValueChange={(v) => pickEngine(v as Engine)}
			>
				<Select.Trigger class="w-full">{ENGINES[form.engine].label}</Select.Trigger>
				<Select.Content>
					{#each Object.entries(ENGINES) as [value, spec] (value)}
						<Select.Item {value} label={spec.label} />
					{/each}
				</Select.Content>
			</Select.Root>
			{#if ENGINES[form.engine].server}
				<div class="grid grid-cols-3 gap-3">
					<Input class="col-span-2" placeholder="Host" bind:value={form.host} />
					<Input type="number" placeholder="Port" bind:value={form.port} />
				</div>
				<Input
					placeholder={ENGINES[form.engine].blankDatabase
						? `Database (${ENGINES[form.engine].blankDatabase} if blank)`
						: 'Database'}
					bind:value={form.database}
				/>
				<Input placeholder="Username" bind:value={form.username} />
				<Input
					type="password"
					placeholder={form.id ? 'Password (unchanged if empty)' : 'Password'}
					bind:value={form.password}
				/>
			{:else}
				<div class="flex gap-2">
					<Input
						class="grow font-mono text-xs"
						placeholder="/home/you/data.db"
						bind:value={form.database}
					/>
					<Button variant="outline" onclick={browseSqlite}>
						<FolderOpenIcon class="size-4" /> Browse
					</Button>
				</div>
				<p class="text-xs text-muted-foreground">
					Pick the file with Browse, or paste a full path starting from the root
					(<span class="font-mono">/home/…</span> or <span class="font-mono">C:\…</span>) — a
					relative path is resolved against the app's working directory, not yours.
				</p>
			{/if}
		</div>
		<Dialog.Footer>
			<Button variant="outline" disabled={busy} onclick={test}>Test</Button>
			<Button
				disabled={busy || !form.name || (!form.database && !ENGINES[form.engine].blankDatabase)}
				onclick={save}>Save</Button
			>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
