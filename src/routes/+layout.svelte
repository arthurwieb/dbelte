<script lang="ts">
	import './layout.css';

	import { Toaster } from '$lib/components/ui/sonner';
	import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
	import TitleBar from '$lib/components/TitleBar.svelte';
	import ResizeGrips from '$lib/components/ResizeGrips.svelte';

	let { children } = $props();
</script>

<svelte:head><link rel="icon" href="/logo.png" /></svelte:head>
<Toaster position="bottom-right" />
<ConfirmDialog />

<div class="flex h-screen flex-col">
	<TitleBar />
	<div class="min-h-0 flex-1 overflow-auto">
		{@render children()}
	</div>
</div>
<ResizeGrips />
