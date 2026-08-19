<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import { EditorView, basicSetup } from 'codemirror';
	import { keymap } from '@codemirror/view';
	import { Compartment, Prec } from '@codemirror/state';
	import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
	import { tags as t } from '@lezer/highlight';
	import { sql, PostgreSQL, SQLite } from '@codemirror/lang-sql';

	let {
		value = $bindable(''),
		engine = 'postgres',
		schema = {},
		onrun
	}: {
		value?: string;
		engine?: 'postgres' | 'sqlite';
		schema?: Record<string, string[]>;
		onrun?: () => void;
	} = $props();

	// basicSetup ships CodeMirror's *light* palette — keywords come out #708, a
	// near-black purple that disappears against our background. These are the
	// tags @codemirror/lang-sql actually emits.
	const highlight = HighlightStyle.define([
		{ tag: t.keyword, color: '#c792ff', fontWeight: '500' },
		{ tag: t.typeName, color: '#ffd479' },
		{ tag: t.string, color: '#8ce99a' },
		{ tag: t.number, color: '#ffab70' },
		{ tag: [t.bool, t.null], color: '#ff7b72' },
		{ tag: t.operator, color: '#c9c9c9' },
		{ tag: t.name, color: 'var(--foreground)' },
		{ tag: [t.lineComment, t.blockComment], color: '#7a7a7a', fontStyle: 'italic' }
	]);

	const langCompartment = new Compartment();
	let view: EditorView;
	let container: HTMLDivElement;

	const theme = EditorView.theme(
		{
			'&': {
				backgroundColor: 'var(--background)',
				color: 'var(--foreground)',
				fontSize: '13px',
				height: '100%'
			},
			'.cm-content': { fontFamily: 'var(--font-mono)', caretColor: 'var(--primary)' },
			'.cm-cursor': { borderLeftColor: 'var(--primary)' },
			'.cm-gutters': {
				backgroundColor: 'var(--card)',
				color: 'var(--muted-foreground)',
				border: 'none'
			},
			'.cm-activeLine': { backgroundColor: 'color-mix(in oklab, var(--muted) 40%, transparent)' },
			'.cm-activeLineGutter': { backgroundColor: 'var(--muted)' },
			'&.cm-focused .cm-selectionBackground, .cm-selectionBackground': {
				backgroundColor: 'color-mix(in oklab, var(--primary) 30%, transparent) !important'
			},
			'.cm-tooltip': {
				backgroundColor: 'var(--popover)',
				color: 'var(--popover-foreground)',
				border: '1px solid var(--border)'
			},
			'.cm-tooltip-autocomplete ul li[aria-selected]': {
				backgroundColor: 'var(--primary)',
				color: 'var(--primary-foreground)'
			}
		},
		{ dark: true }
	);

	function langExt(engine: string, schema: Record<string, string[]>) {
		return sql({ dialect: engine === 'sqlite' ? SQLite : PostgreSQL, schema, upperCaseKeywords: true });
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
					}
				]),
				basicSetup,
				// Prec.high so it beats the default style basicSetup pulls in
				Prec.high(syntaxHighlighting(highlight)),
				langCompartment.of(langExt(engine, schema)),
				theme,
				EditorView.updateListener.of((u) => {
					if (u.docChanged) value = u.state.doc.toString();
				})
			]
		});
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
