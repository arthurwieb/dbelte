import { clsx, type ClassValue } from 'clsx';
import { twMerge } from 'tailwind-merge';
import type { TableRef } from '$lib/api';

export function cn(...inputs: ClassValue[]) {
	return twMerge(clsx(inputs));
}

/** What the user sees: the schema is noise until it isn't the default one. */
export function tableLabel(t: TableRef): string {
	return t.schema && t.schema !== 'public' ? `${t.schema}.${t.name}` : t.name;
}

/** `"reporting"."orders"` — for SQL we hand to the Query tab. */
export function quoteTable(t: TableRef): string {
	const q = (s: string) => `"${s.replaceAll('"', '""')}"`;
	return t.schema ? `${q(t.schema)}.${q(t.name)}` : q(t.name);
}

export type WithElementRef<T, U extends HTMLElement = HTMLElement> = T & { ref?: U | null };

export type WithoutChild<T> = T extends { child?: unknown } ? Omit<T, 'child'> : T;
export type WithoutChildren<T> = T extends { children?: unknown } ? Omit<T, 'children'> : T;
export type WithoutChildrenOrChild<T> = WithoutChildren<WithoutChild<T>>;
