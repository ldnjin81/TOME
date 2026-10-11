import { isValidElement } from 'react';
import { afterEach, describe, expect, it } from 'vitest';
import { dictionary } from './i18n.data';
import { getLanguage, setLanguage, systemLanguage, t, tn } from './i18n';
import { explainError } from './errors';
import ts from 'typescript';

afterEach(() => setLanguage(''));

describe('화면 언어', () => {
  it('사전의 모든 항목에 한국어와 영어 문구가 있다', () => {
    expect(Object.keys(dictionary).length).toBeGreaterThan(380);
    for (const [key, value] of Object.entries(dictionary)) {
      expect(value.ko.trim(), key).not.toBe('');
      expect(value.en.trim(), key).not.toBe('');
      // A key may carry a context after @@ when the same Korean needs different English.
      expect(value.ko, key).toBe(key.split('@@')[0]);
      expect(value.en, key).not.toMatch(/[가-힣]/);
    }
  });

  it('화면 코드에서 참조하는 정적 문구가 사전에 있다', () => {
    const sources = { ...import.meta.glob('./*.tsx', { query: '?raw', import: 'default', eager: true }), ...import.meta.glob('./errors.ts', { query: '?raw', import: 'default', eager: true }) } as Record<string, string>;
    for (const [file, raw] of Object.entries(sources)) {
      const source = ts.createSourceFile(file, raw, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
      function visit(node: ts.Node) {
        if (ts.isCallExpression(node) && (node.expression.getText(source) === 't' || node.expression.getText(source) === 'translate') && ts.isStringLiteral(node.arguments[0])) {
          const key = node.arguments[0].text;
          expect(Object.prototype.hasOwnProperty.call(dictionary, key), `${file}: ${key}`).toBe(true);
        }
        ts.forEachChild(node, visit);
      }
      visit(source);
    }
  });

  it('언어를 전환하고 문장의 매개변수를 치환한다', () => {
    setLanguage('ko');
    expect(t('충돌 {p0}개 남음', { p0: 2 })).toBe('충돌 2개 남음');
    setLanguage('en');
    expect(t('충돌 {p0}개 남음', { p0: 2 })).toBe('2 conflicts left');
    expect(t('충돌 {p0}개 남음', { p0: 0 })).toBe('0 conflicts left');
  });

  it('시스템 언어를 한국어 접두사로 판정하고 명시 설정을 우선한다', () => {
    expect(systemLanguage('ko-KR')).toBe('ko');
    expect(systemLanguage('KO')).toBe('ko');
    expect(systemLanguage('en-US')).toBe('en');
    expect(systemLanguage('ja-JP')).toBe('en');
    setLanguage('en');
    expect(getLanguage()).toBe('en');
  });

  it('영어 오류 설명과 원문 표기를 표시한다', () => {
    setLanguage('en');
    expect(explainError('Branch has diverged')).toContain('The server has new revisions');
    expect(explainError('Branch has diverged')).toContain('(original: Branch has diverged)');
    expect(explainError('No restack in progress')).toContain('No restack in progress');
    expect(explainError('No restack in progress')).not.toMatch(/[가-힣]/);
  });
});

describe('복수형과 문장 조립', () => {
  it('영어는 숫자에 맞춰 단수·복수를 고르고, 한국어는 그대로', () => {
    setLanguage('en');
    expect(t('미커밋 변경 {p0}개', { p0: 1 })).toBe('1 uncommitted change');
    expect(t('미커밋 변경 {p0}개', { p0: 3 })).toBe('3 uncommitted changes');
    expect(t('{p0} · 잠금 {p1}개 · 작업자 {p2}명', { p0: 'main', p1: 1, p2: 2 })).toBe('main · 1 lock · 2 owners');
    expect(t('↓ {p0} 리비전', { p0: 1 })).toBe('↓ 1 new revision');
    setLanguage('ko');
    expect(t('미커밋 변경 {p0}개', { p0: 1 })).toBe('미커밋 변경 1개');
    expect(t('{p0}님이 잠금', { p0: 'kim' })).toBe('kim님이 잠금');
    expect(t('{p0}의 현재 리비전에서 시작합니다.', { p0: 'main' })).toBe('main의 현재 리비전에서 시작합니다.');
  });

  it('tn은 문장 속 자리에 요소를 끼운다(언어마다 어순이 달라도)', () => {
    const text = (nodes: ReturnType<typeof tn>) => nodes.map((n) => (isValidElement(n) ? String((n.props as { children: unknown }).children) : String(n))).join('');
    const params = { p0: 'A', p1: '!', p2: 'B' };
    setLanguage('ko');
    expect(text(tn('한 줄에 글롭 하나. 그냥 쓰면 {p0}, {p1}로 시작하면 다시 {p2}합니다. 비우면 전부 받습니다.', params))).toBe('한 줄에 글롭 하나. 그냥 쓰면 A, !로 시작하면 다시 B합니다. 비우면 전부 받습니다.');
    setLanguage('en');
    expect(text(tn('한 줄에 글롭 하나. 그냥 쓰면 {p0}, {p1}로 시작하면 다시 {p2}합니다. 비우면 전부 받습니다.', params))).toBe('One glob per line. A plain line is A; a line starting with ! is B again. Leave it empty to get everything.');
    setLanguage('');
  });

  it('영어 문구에 콜론 뒤 공백 빠짐·소문자 시작 같은 흔한 실수가 없다', () => {
    for (const [key, entry] of Object.entries(dictionary)) {
      expect(entry.en, key).not.toMatch(/:\{p\d\}/);
      expect(entry.en, key).not.toMatch(/\b(items|Please|I'm|I am)\b/);
    }
  });
});
