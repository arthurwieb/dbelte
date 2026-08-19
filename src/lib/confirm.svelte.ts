// Drop-in replacement for @tauri-apps/plugin-dialog's confirm(), rendered by
// <ConfirmDialog /> in the root layout so it inherits the app theme.

interface Pending {
	message: string;
	title: string;
	okLabel: string;
	resolve: (ok: boolean) => void;
}

export const confirmState = $state<{ pending: Pending | null }>({ pending: null });

export function confirm(
	message: string,
	opts: { title?: string; okLabel?: string } = {}
): Promise<boolean> {
	// a second prompt while one is open cancels the first
	confirmState.pending?.resolve(false);
	return new Promise((resolve) => {
		confirmState.pending = {
			message,
			title: opts.title ?? 'Are you sure?',
			okLabel: opts.okLabel ?? 'Delete',
			resolve
		};
	});
}

export function answer(ok: boolean) {
	confirmState.pending?.resolve(ok);
	confirmState.pending = null;
}
