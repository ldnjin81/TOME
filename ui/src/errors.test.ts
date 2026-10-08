import { describe, expect, it } from 'vitest';
import { explainError } from './errors';

describe('explainError', () => {
  it('알려진 Lore 오류는 한국어 안내와 짧은 원문', () => {
    const shown = explainError('Branch has diverged, sync to merge remote changes');
    expect(shown).toMatch(/^서버에 새 리비전이 있어 push할 수 없습니다/);
    expect(shown).toContain('(원문: Branch has diverged');
  });

  it('앱이 붙인 "… 실패: " 머리는 앞에 남긴다', () => {
    expect(explainError('main push 실패: Not authorized to access repository')).toMatch(/^main push 실패: 이 저장소에 접근할 권한이 없습니다/);
  });

  it('연결 오류는 여러 표현을 같은 안내로', () => {
    for (const raw of ['Disconnected from server', 'transport error: connection refused', 'Not connected to remote: timed out']) {
      expect(explainError(raw)).toMatch(/^Lore 서버에 연결할 수 없습니다/);
    }
  });

  it('잠금·충돌·작업본', () => {
    expect(explainError('resource locked by somebody else')).toMatch(/^다른 사람이 잠근 파일입니다/);
    expect(explainError('Unable to commit when Content/A.uasset is still in conflict')).toMatch(/^아직 정하지 않은 충돌 파일/);
    expect(explainError('not a Lore working copy: C:\\x')).toMatch(/^Lore 작업본 폴더가 아닙니다/);
  });

  it('모르는 오류나 한국어 메시지는 그대로', () => {
    expect(explainError('알 수 없는 문제')).toBe('알 수 없는 문제');
    expect(explainError('something odd happened')).toBe('something odd happened');
    expect(explainError('')).toBe('');
  });

  it('긴 원문은 잘라서 보인다', () => {
    const shown = explainError('Not authorized ' + 'x'.repeat(400));
    expect(shown.length).toBeLessThan(330);
    expect(shown).toContain('…)');
  });
});
