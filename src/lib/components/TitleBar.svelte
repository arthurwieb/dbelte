<script lang="ts">
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import MinusIcon from '@lucide/svelte/icons/minus';
	import SquareIcon from '@lucide/svelte/icons/square';
	import CopyIcon from '@lucide/svelte/icons/copy';
	import XIcon from '@lucide/svelte/icons/x';

	const win = getCurrentWindow();
	let maximized = $state(false);

	async function toggle() {
		await win.toggleMaximize();
		maximized = await win.isMaximized();
	}
</script>

<!-- data-tauri-drag-region makes the strip behave like a native title bar (drag,
     double-click to maximize); buttons opt out so clicks aren't swallowed -->
<header
	data-tauri-drag-region
	class="flex h-9 shrink-0 select-none items-center justify-between border-b bg-card pl-3"
>
	<span data-tauri-drag-region class="flex items-center gap-2 text-xs font-semibold tracking-wide">
		<img src="/logo.png" alt="" class="size-4" />
		<span><span class="text-primary">db</span>elte</span>
	</span>
	<div class="flex h-full">
		<button
			class="flex h-full w-11 items-center justify-center text-muted-foreground hover:bg-muted hover:text-foreground"
			title="Minimize"
			onclick={() => win.minimize()}
		>
			<MinusIcon class="size-3.5" />
		</button>
		<button
			class="flex h-full w-11 items-center justify-center text-muted-foreground hover:bg-muted hover:text-foreground"
			title={maximized ? 'Restore' : 'Maximize'}
			onclick={toggle}
		>
			{#if maximized}
				<CopyIcon class="size-3" />
			{:else}
				<SquareIcon class="size-3" />
			{/if}
		</button>
		<button
			class="flex h-full w-11 items-center justify-center text-muted-foreground hover:bg-destructive hover:text-white"
			title="Close"
			onclick={() => win.close()}
		>
			<XIcon class="size-3.5" />
		</button>
	</div>
</header>
