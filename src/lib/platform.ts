// The *bindings* are already portable: CodeMirror's `Mod-` prefix resolves to
// Cmd on macOS and Ctrl everywhere else, and Alt is Option on a Mac keyboard.
// This is purely about labelling them with symbols the user recognises.
export const isMac = navigator.userAgent.includes('Mac');

export const KEYS = {
	run: isMac ? '⌘↵' : 'Ctrl+Enter',
	format: isMac ? '⇧⌥F' : 'Shift+Alt+F'
};
