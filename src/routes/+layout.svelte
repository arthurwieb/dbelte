<script lang="ts">
	import './layout.css';

	import { onMount } from 'svelte';

	import { Toaster } from '$lib/components/ui/sonner';
	import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
	import TitleBar from '$lib/components/TitleBar.svelte';
	import ResizeGrips from '$lib/components/ResizeGrips.svelte';

	let { children } = $props();

	// Hand off from the static splash in app.html: by the time this runs the
	// stylesheet and the app shell have painted, so there is nothing left to
	// cover. Removed on a timer rather than transitionend, which never fires if
	// the transition is skipped and would leave the splash on top forever.
	onMount(() => {
		const splash = document.getElementById('splash');
		if (!splash) return;
		splash.classList.add('done');
		setTimeout(() => splash.remove(), 150); // matches the CSS transition
	});
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
