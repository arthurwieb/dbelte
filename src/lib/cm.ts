// Shared CodeMirror look, used by the SQL editor and the JSON cell editor.
// basicSetup ships CodeMirror's *light* palette — keywords come out #708, a
// near-black purple that disappears against our background.
import { EditorView } from '@codemirror/view';
import { HighlightStyle } from '@codemirror/language';
import { tags as t } from '@lezer/highlight';

/** Covers the tags @codemirror/lang-sql and @codemirror/lang-json emit. */
export const cmHighlight = HighlightStyle.define([
	{ tag: t.keyword, color: '#c792ff', fontWeight: '500' },
	{ tag: t.typeName, color: '#ffd479' },
	{ tag: t.string, color: '#8ce99a' },
	{ tag: t.number, color: '#ffab70' },
	{ tag: [t.bool, t.null], color: '#ff7b72' },
	{ tag: t.operator, color: '#c9c9c9' },
	{ tag: t.name, color: 'var(--foreground)' },
	// JSON object keys — propertyName, distinct from the string values
	{ tag: [t.propertyName, t.definition(t.propertyName)], color: '#7cc7ff' },
	{ tag: [t.lineComment, t.blockComment], color: '#7a7a7a', fontStyle: 'italic' }
]);

export const cmTheme = EditorView.theme(
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
