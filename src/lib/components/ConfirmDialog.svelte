<script lang="ts">
	import { confirmState, answer } from '$lib/confirm.svelte';
	import { Button } from '$lib/components/ui/button';
	import * as Dialog from '$lib/components/ui/dialog';

	const pending = $derived(confirmState.pending);
</script>

<Dialog.Root open={!!pending} onOpenChange={(o: boolean) => !o && answer(false)}>
	<Dialog.Content class="sm:max-w-sm">
		<Dialog.Header>
			<Dialog.Title>{pending?.title}</Dialog.Title>
			<Dialog.Description class="text-sm whitespace-pre-line">{pending?.message}</Dialog.Description>
		</Dialog.Header>
		<Dialog.Footer>
			<Button size="sm" variant="outline" onclick={() => answer(false)}>Cancel</Button>
			<Button size="sm" variant="destructive" onclick={() => answer(true)}>
				{pending?.okLabel}
			</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
