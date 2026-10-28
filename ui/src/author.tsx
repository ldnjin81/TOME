import type { CSSProperties } from 'react';
import { t } from './i18n';

/** 이름마다 고정된 색상(0~359). 같은 사람은 어느 화면에서나 같은 색이다(FNV-1a). */
export function authorHue(name: string): number {
  let hash = 0x811c9dc5;
  for (let i = 0; i < name.length; i++) {
    hash ^= name.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return hash % 360;
}

/** 작성자·잠금 소유자 이름. 색 점과 색 글자로 사람을 구분하고, 내 이름은 "나"로 보인다. */
export function Author({ name, me, label }: { name: string; me?: string; label?: string }) {
  if (!name) return null;
  const text = label ?? (me && name === me ? t('나') : name);
  return <span className="author" style={{ '--author-hue': authorHue(name) } as CSSProperties} title={name}>{text}</span>;
}
