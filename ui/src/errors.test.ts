import { beforeEach, afterEach, describe, expect, it } from 'vitest';
import { explainError, needsLogin } from './errors';
import { setLanguage } from './i18n';

beforeEach(() => setLanguage('ko'));
afterEach(() => setLanguage(''));

describe('explainError', () => {
  it('알려진 Lore 오류는 한국어 안내와 짧은 원문', () => {
    const shown = explainError('Branch has diverged, sync to merge remote changes');
    expect(shown).toMatch(/^서버에 새 리비전이 있어 push할 수 없습니다/);
    expect(shown).toContain('(원문: Branch has diverged');
  });

  it('보관 브랜치 복원 오류는 복원 안내로, 원인 문구보다 먼저 맞춘다', () => {
    expect(explainError('A branch named feat already exists; rename or archive it before restoring')).toMatch(/^같은 이름의 활성 브랜치가 있어 복원할 수 없습니다/);
    expect(explainError('Restored feat locally, but could not push it: Disconnected from server')).toMatch(/^이 작업본에서는 복원했지만 서버에 올리지 못했습니다/);
    expect(explainError('Restored feat, but could not return to main: x')).toMatch(/^복원은 했지만 마무리하지 못했습니다/);
    expect(explainError('Branch feat is not archived')).toMatch(/^이미 보관이 풀린 브랜치입니다/);
    expect(explainError('Restoring a branch needs the server; go online first')).toMatch(/^브랜치 복원에는 서버 연결이 필요합니다/);
    // Lore가 직접 낸 이름 충돌은 기존 안내 그대로.
    expect(explainError('Branch feat already exists, use switch instead')).toMatch(/^같은 이름의 브랜치가 이미 있습니다/);
  });

  it('앱이 붙인 "… 실패: " 머리는 앞에 남긴다', () => {
    expect(explainError('main push 실패: Not authorized to access repository')).toMatch(/^main push 실패: 권한 없음/);
  });

  it('인증 누락과 만료는 로그인 안내로 연결한다', () => {
    for (const raw of ['NotAuthenticated', 'TokenNotFound', 'authorization header required', 'Not authenticated', 'No token stored', 'push 실패: Not authenticated']) {
      expect(needsLogin(raw), raw).toBe(true);
      expect(explainError(raw), raw).toContain('로그인이 필요하거나 만료되었습니다');
    }
  });

  it('권한 부족과 연결 실패에는 로그인 버튼을 붙이지 않는다', () => {
    for (const raw of ['NotAuthorized', 'Not authorized', 'connection refused', 'Disconnected from server']) {
      expect(needsLogin(raw), raw).toBe(false);
    }
    expect(explainError('NotAuthorized')).toContain('권한 없음');
  });

  it('영어에서도 인증 오류와 권한 부족을 구분한다', () => {
    setLanguage('en');
    expect(explainError('NotAuthenticated')).toContain('Login is required or has expired');
    expect(explainError('NotAuthorized')).toContain('Permission denied');
  });

  it('장치 승인 거부와 승인 시간 초과를 구분한다', () => {
    expect(explainError('expired_token')).toContain('승인 시간이 지났습니다');
    expect(explainError('Login session expired before it was approved')).toContain('승인 시간이 지났습니다');
    expect(explainError('access_denied')).toContain('로그인 요청이 거부되었습니다');
    expect(explainError('Login already in progress')).toContain('이전 로그인 요청이 끝나기를 기다리는 중');
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
