import { describe, expect, it } from 'vitest';
import { suggestTarget, targetUnder } from './InstanceDialog';

describe('작업본 이동 대상', () => {
  it('같은 폴더에 새 이름을 제안한다', () => {
    expect(suggestTarget('C:\\Project\\Game')).toBe('C:\\Project\\Game-moved');
    expect(suggestTarget('/work/game/')).toBe('/work/game-moved');
    expect(suggestTarget('')).toBe('');
  });

  it('고른 폴더 아래에 현재 작업본 이름을 붙인다', () => {
    expect(targetUnder('D:\\Work', 'C:\\Project\\Game')).toBe('D:\\Work\\Game');
    expect(targetUnder('D:\\Work\\', 'C:\\Project\\Game\\')).toBe('D:\\Work\\Game');
    expect(targetUnder('/mnt/work', '/home/me/game')).toBe('/mnt/work/game');
  });
});
