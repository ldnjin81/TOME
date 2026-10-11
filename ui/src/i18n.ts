import { useSyncExternalStore } from 'react';
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

export function t(key: string, params?: Record<string, string | number | boolean | null | undefined>): string {
  const entry = dictionary[key as keyof typeof dictionary];
  const text: string = entry ? entry[getLanguage()] : key;
  return params ? text.replace(/\{(\w+)\}/g, (whole, name: string) =>
    Object.prototype.hasOwnProperty.call(params, name) ? String(params[name] ?? '') : whole) : text;
}
