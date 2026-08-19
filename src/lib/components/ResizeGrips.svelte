<script lang="ts">
	import { getCurrentWindow } from '@tauri-apps/api/window';

	// @tauri-apps/api declares ResizeDirection but doesn't export it
	type ResizeDirection = Parameters<
		ReturnType<typeof getCurrentWindow>['startResizeDragging']
	>[0];

	// An undecorated window has no WM resize border (notably on Linux), so we
	// draw our own invisible edge/corner grips and drive the resize ourselves.
	const win = getCurrentWindow();
	const GRIPS: [ResizeDirection, string][] = [
		['North', 'inset-x-2 top-0 h-1 cursor-n-resize'],
		['South', 'inset-x-2 bottom-0 h-1 cursor-s-resize'],
		['West', 'inset-y-2 left-0 w-1 cursor-w-resize'],
		['East', 'inset-y-2 right-0 w-1 cursor-e-resize'],
		['NorthWest', 'top-0 left-0 size-2 cursor-nw-resize'],
		['NorthEast', 'top-0 right-0 size-2 cursor-ne-resize'],
		['SouthWest', 'bottom-0 left-0 size-2 cursor-sw-resize'],
		['SouthEast', 'bottom-0 right-0 size-2 cursor-se-resize']
	];
</script>

{#each GRIPS as [dir, cls] (dir)}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="fixed z-50 {cls}" onpointerdown={() => win.startResizeDragging(dir)}></div>
{/each}
