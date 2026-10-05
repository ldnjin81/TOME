import { describe, expect, it } from 'vitest';
import { virtualGrid } from './assetVirtual';

describe('virtualGrid', () => {
  it('2000개에서도 화면 주변 카드만 포함한다', () => {
    const range = virtualGrid(2000, 800, 128, 0, 600, 0);
    expect(range.columns).toBe(5);
    expect(range.end - range.first).toBe(25);
    expect(range.end - range.first).toBeLessThan(100);
    expect(range.previewEnd).toBeGreaterThan(range.end);
  });

  it('스크롤하면 앞 공간과 범위가 이동하고 전체 높이는 유지된다', () => {
    const top = virtualGrid(2000, 800, 128, 0, 600, 0);
    const lower = virtualGrid(2000, 800, 128, 3000, 600, 0);
    expect(lower.first).toBeGreaterThan(top.first);
    expect(lower.end - lower.first).toBe(40);
    expect(lower.top).toBeGreaterThan(0);
    expect(lower.previewFirst).toBeLessThan(lower.first);
    expect(lower.top + lower.bottom + Math.ceil((lower.end - lower.first) / lower.columns) * lower.rowHeight + (Math.ceil((lower.end - lower.first) / lower.columns) - 1) * 10)
      .toBe(top.top + top.bottom + Math.ceil((top.end - top.first) / top.columns) * top.rowHeight + (Math.ceil((top.end - top.first) / top.columns) - 1) * 10);
  });

  it('썸네일 크기와 거른 개수를 반영한다', () => {
    expect(virtualGrid(20, 800, 224, 0, 600, 0).columns).toBe(3);
    const range = virtualGrid(3, 800, 128, 0, 600, 0);
    expect(range.first).toBe(0);
    expect(range.end).toBe(3);
    expect(range.bottom).toBe(0);
  });
});
