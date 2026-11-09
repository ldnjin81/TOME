import { describe, expect, it } from 'vitest';
import { viewRules, viewSummary } from './viewLogic';
import { joinUnder, nestedRootOf } from './nestedLogic';

const presets = [
  { name: '전체', lines: [] },
  { name: '콘텐츠 제외', lines: ['/Content/'] },
];

describe('View 알약 요약', () => {
  it('규칙이 없으면 전체', () => {
    expect(viewSummary([], presets)).toEqual({ kind: 'all' });
    expect(viewSummary(['', '   '], presets)).toEqual({ kind: 'all' });
  });

  it('프리셋과 같으면 그 이름, 아니면 규칙 수', () => {
    expect(viewSummary([' /Content/ ', ''], presets)).toEqual({ kind: 'preset', name: '콘텐츠 제외' });
    expect(viewSummary(['/*', '!/Source/'], presets)).toEqual({ kind: 'rules', count: 2 });
    expect(viewRules(['a', ' ', ' b '])).toEqual(['a', 'b']);
  });
});

describe('중첩 저장소 경로', () => {
  it('루트와 같거나 그 아래만 맞춘다', () => {
    const roots = ['Plugins/Sub'];
    expect(nestedRootOf('Plugins/Sub', roots)).toBe('Plugins/Sub');
    expect(nestedRootOf('Plugins\\Sub\\a.txt', roots)).toBe('Plugins/Sub');
    expect(nestedRootOf('Plugins/Sub/', roots)).toBe('Plugins/Sub');
    expect(nestedRootOf('Plugins/Subway/a.txt', roots)).toBeUndefined();
    expect(nestedRootOf('a.txt', [])).toBeUndefined();
  });

  it('작업본 경로 아래로 붙인다', () => {
    expect(joinUnder('C:\\Project\\Game\\', 'Plugins/Sub')).toBe('C:\\Project\\Game\\Plugins\\Sub');
    expect(joinUnder('/work/game', 'Plugins/Sub')).toBe('/work/game/Plugins/Sub');
  });
});
