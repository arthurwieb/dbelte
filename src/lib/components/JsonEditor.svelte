<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import { EditorView, basicSetup } from 'codemirror';
	import { keymap } from '@codemirror/view';
	import { EditorState, Prec } from '@codemirror/state';
	import { syntaxHighlighting } from '@codemirror/language';
	import { json } from '@codemirror/lang-json';
	import { cmHighlight, cmTheme } from '$lib/cm';

	let {
		value = $bindable(''),
		readonly = false,
		onsave
	}: { value?: string; readonly?: boolean; onsave?: () => void } = $props();

	let view: EditorView;
	let container: HTMLDivElement;

	onMount(() => {
		view = new EditorView({
			doc: untrack(() => value),
			parent: container,
			extensions: [
				keymap.of([
					{
						key: 'Mod-Enter',
						run: () => {
							onsave?.();
							return true;
						}
					}
				]),
				basicSetup,
				// Prec.high so it beats the light palette basicSetup pulls in
				Prec.high(syntaxHighlighting(cmHighlight)),
				json(),
				cmTheme,
				EditorState.readOnly.of(readonly),
				EditorView.updateListener.of((u) => {
					if (u.docChanged) value = u.state.doc.toString();
				})
			]
		});
		view.focus();
		return () => view.destroy();
	});
</script>

<div class="h-full overflow-hidden rounded-lg border" bind:this={container}></div>
