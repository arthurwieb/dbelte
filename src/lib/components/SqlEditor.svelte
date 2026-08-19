<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import { EditorView, basicSetup } from 'codemirror';
	import { keymap } from '@codemirror/view';
	import { Compartment, Prec } from '@codemirror/state';
	import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
	import { tags as t } from '@lezer/highlight';
	import { sql, PostgreSQL, SQLite } from '@codemirror/lang-sql';
	import { format as formatSql } from 'sql-formatter';

	let {
		value = $bindable(''),
		engine = 'postgres',
		schema = {},
		onrun,
		api = $bindable()
	}: {
		value?: string;
		engine?: 'postgres' | 'sqlite';
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

	/** Pretty-print the buffer in place, keeping the cursor from jumping about. */
	function format() {
		if (!view) return;
		const src = view.state.doc.toString();
		if (!src.trim()) return;
		let out: string;
		try {
			out = formatSql(src, {
				language: engine === 'sqlite' ? 'sqlite' : 'postgresql',
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
				Prec.high(syntaxHighlighting(highlight)),
				langCompartment.of(langExt(engine, schema)),
				theme,
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
