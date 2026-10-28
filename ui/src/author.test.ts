import { describe, expect, it } from 'vitest';
import { authorHue } from './author';

describe('작성자 색상', () => {
  it('같은 이름은 늘 같은 색이다', () => {
    expect(authorHue('ldnjin')).toBe(authorHue('ldnjin'));
    expect(authorHue('agent-pc')).toBeGreaterThanOrEqual(0);
    expect(authorHue('agent-pc')).toBeLessThan(360);
  });
  it('흔한 이름들이 서로 다른 색으로 갈린다', () => {
    const names = ['ldnjin', 'agent-pc', 'agent-mac', 'kim-pc', 'kim-laptop', 'codex'];
    expect(new Set(names.map(authorHue)).size).toBe(names.length);
  });
});
