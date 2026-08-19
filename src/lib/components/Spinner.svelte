<script lang="ts">
	import LoaderCircleIcon from '@lucide/svelte/icons/loader-circle';
	import { cn } from '$lib/utils';

	let { class: className }: { class?: string } = $props();
</script>

<LoaderCircleIcon class={cn('size-4 shrink-0 animate-spin text-primary', className)} />
