import { createElement, Fragment, useSyncExternalStore, type ReactNode } from 'react';
import { dictionary } from './i18n.data';

export type LanguageSetting = '' | 'ko' | 'en';
export type Language = 'ko' | 'en';

let setting: LanguageSetting = '';
const listeners = new Set<() => void>();

export function systemLanguage(value = typeof navigator === 'undefined' ? 'ko' : navigator.language): Language {
  return value.toLowerCase().startsWith('ko') ? 'ko' : 'en';
}

export function getLanguage(): Language {
  return setting || systemLanguage();
}

export function getLocale(): string {
  return getLanguage() === 'ko' ? 'ko-KR' : 'en-US';
}

export function setLanguage(value: LanguageSetting): void {
  setting = value;
  if (typeof document !== 'undefined') document.documentElement.lang = getLanguage();
  listeners.forEach(listener => listener());
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function useLanguage(): Language {
  return useSyncExternalStore(subscribe, getLanguage, () => 'ko');
}

type Params = Record<string, string | number | boolean | null | undefined>;

function lookup(key: string): string {
  const entry = dictionary[key as keyof typeof dictionary];
  return entry ? entry[getLanguage()] : key;
}

/** `{name|one|other}` picks the form for the number in `name` (English plurals). */
function plurals(text: string, params?: Record<string, unknown>): string {
  return text.replace(/\{(\w+)\|([^|}]*)\|([^}]*)\}/g, (_whole, name: string, one: string, other: string) =>
    Number(params?.[name]) === 1 ? one : other);
}

export function t(key: string, params?: Params): string {
  const text = plurals(lookup(key), params);
  return params ? text.replace(/\{(\w+)\}/g, (whole, name: string) =>
    Object.prototype.hasOwnProperty.call(params, name) ? String(params[name] ?? '') : whole) : text;
}

/** Like `t`, with elements (bold, code, a span) placed where the sentence names them. */
export function tn(key: string, params: Record<string, ReactNode>): ReactNode[] {
  const text = plurals(lookup(key), params);
  return text.split(/(\{\w+\})/).map((part, i) => {
    const name = /^\{(\w+)\}$/.exec(part)?.[1];
    const node = name && Object.prototype.hasOwnProperty.call(params, name) ? params[name] : part;
    return createElement(Fragment, { key: i }, node);
  });
}
