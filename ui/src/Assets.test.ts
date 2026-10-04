import { describe, expect, it } from 'vitest';
import { assetMarks, formatSize } from './Assets';
import { classTone } from './assetLogic';
import type { ChangedFile, Lock } from './types';

const file = (path: string, directory = false): ChangedFile => ({
  path, directory, action: 'modify', staged: false, conflict: false, unresolved: false, resolution: '',
});
const lock = (path: string, owner: string): Lock => ({ path, owner, locked_at: 0 });

describe('assetMarks', () => {
  it('디렉터리는 변경 표시에서 제외하고 파일만 남긴다', () => {
    const marks = assetMarks([file('Content', true), file('Content/A.uasset')], []);
    expect([...marks.changed.keys()]).toEqual(['Content/A.uasset']);
  });

  it('경로로 잠금과 변경 내용을 찾는다', () => {
    const changed = file('Content/A.uasset');
    const locked = lock('Content/A.uasset', 'alice');
    const marks = assetMarks([changed], [locked]);
    expect(marks.changed.get(changed.path)).toBe(changed);
    expect(marks.locks.get(locked.path)).toBe(locked);
  });

  it('표시할 항목이 없으면 빈 맵을 만든다', () => {
    const marks = assetMarks([], []);
    expect(marks.changed.size).toBe(0);
    expect(marks.locks.size).toBe(0);
  });
});

describe('formatSize', () => {
  it('1 KB 미만은 바이트로 표시한다', () => {
    expect(formatSize(0)).toBe('0 B');
    expect(formatSize(1023)).toBe('1023 B');
  });

  it('KB 범위는 정수로 반올림한다', () => {
    expect(formatSize(1024)).toBe('1 KB');
    expect(formatSize(1536)).toBe('2 KB');
  });

  it('MB 범위는 소수점 한 자리로 표시한다', () => {
    expect(formatSize(1024 * 1024)).toBe('1.0 MB');
    expect(formatSize(1.25 * 1024 * 1024)).toBe('1.3 MB');
  });
});

describe('classTone', () => {
  it('맵 확장자와 World 클래스를 맵으로 분류한다', () => {
    expect(classTone('Unknown', 'umap')).toBe('map');
    expect(classTone('World', 'uasset')).toBe('map');
  });

  it.each([
    ['BlueprintGeneratedClass', 'blueprint'],
    ['MetaSoundSource', 'sound'],
    ['AnimSequence', 'anim'],
    ['Texture2D', 'material'],
    ['DataTable', 'data'],
    ['Unknown', 'other'],
  ])('%s 클래스를 %s 계열로 분류한다', (cls, expected) => {
    expect(classTone(cls, 'uasset')).toBe(expected);
  });

  it('여러 키워드가 겹치면 기존 우선순위를 적용한다', () => {
    expect(classTone('AnimBlueprint', 'uasset')).toBe('blueprint');
  });
});
