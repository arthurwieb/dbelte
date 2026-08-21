<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import { EditorView, basicSetup } from 'codemirror';
	import { keymap } from '@codemirror/view';
	import { Compartment, Prec } from '@codemirror/state';
	import { syntaxHighlighting } from '@codemirror/language';
	import { cmHighlight, cmTheme } from '$lib/cm';
	import { sql } from '@codemirror/lang-sql';
	import { ENGINES } from '$lib/dialect';
	import type { Engine } from '$lib/api';
	import { format as formatSql } from 'sql-formatter';

	let {
		value = $bindable(''),
		engine = 'postgres',
		schema = {},
		onrun,
		api = $bindable()
	}: {
		value?: string;
		engine?: Engine;
		schema?: Record<string, string[]>;
		onrun?: () => void;
		/** Bound out so a parent (the context menu) can drive the editor. */
		api?: EditorApi;
	} = $props();

	export type EditorApi = {
		format: () => void;
		selectAll: () => void;
		clear: () => void;
		focus: () => void;
	};

	const langCompartment = new Compartment();
	let view: EditorView;
	let container: HTMLDivElement;

	/** Pretty-print the buffer in place, keeping the cursor from jumping about. */
	function format() {
		if (!view) return;
		const src = view.state.doc.toString();
		if (!src.trim()) return;
		let out: string;
		try {
			out = formatSql(src, {
				language: ENGINES[engine].formatter,
				keywordCase: 'upper', // matches the editor's upperCaseKeywords completions
				tabWidth: 2
			});
		} catch {
			return; // unparseable mid-edit SQL — leave it alone rather than mangle it
		}
		if (out === src) return;
		replaceAll(out);
	}

	function replaceAll(text: string) {
		view.dispatch({
			changes: { from: 0, to: view.state.doc.length, insert: text },
			selection: { anchor: Math.min(view.state.selection.main.anchor, text.length) }
		});
	}

	function langExt(engine: Engine, schema: Record<string, string[]>) {
		const spec = ENGINES[engine];
		return sql({
			dialect: spec.cm,
			schema,
			// the keys in `schema` have the default schema stripped, so lang-sql
			// has to know which one that was to resolve a qualified name
			defaultSchema: spec.defaultSchema,
			upperCaseKeywords: true
		});
	}

	onMount(() => {
		view = new EditorView({
			doc: untrack(() => value),
			parent: container,
			extensions: [
				keymap.of([
					{
						key: 'Mod-Enter',
						run: () => {
							onrun?.();
							return true;
						}
					},
					{
						key: 'Shift-Alt-f', // the VS Code "format document" binding
						run: () => {
							format();
							return true;
						}
					}
				]),
				basicSetup,
				// Prec.high so it beats the default style basicSetup pulls in
				Prec.high(syntaxHighlighting(cmHighlight)),
				langCompartment.of(langExt(engine, schema)),
				cmTheme,
				EditorView.updateListener.of((u) => {
					if (u.docChanged) value = u.state.doc.toString();
				})
			]
		});
		api = {
			format,
			selectAll: () =>
				view.dispatch({ selection: { anchor: 0, head: view.state.doc.length } }),
			clear: () => replaceAll(''),
			focus: () => view.focus()
		};
		return () => view.destroy();
	});

	// reconfigure language/completions when schema or engine changes
	$effect(() => {
		view?.dispatch({ effects: langCompartment.reconfigure(langExt(engine, schema)) });
	});

	// external value changes (loading a saved query)
	$effect(() => {
		if (view && value !== view.state.doc.toString()) {
			view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: value } });
		}
	});
</script>

<div class="h-full min-h-32 overflow-hidden rounded-lg border" bind:this={container}></div>
