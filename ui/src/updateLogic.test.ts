import { describe, expect, it } from 'vitest';
import { shouldOfferUpdate, type UpdateInfo } from './updateLogic';
import type { Settings } from './types';
const settings = {} as Settings;
const update: UpdateInfo = { available: true, current: '0.3.0', version: '0.4.0', notes: '', date: null, installable: true, check_failed: false };

describe('업데이트 알림 정책', () => {
  it('새 버전을 알리고 같은 버전·이전 버전 결과는 알리지 않는다', () => {
    expect(shouldOfferUpdate(update, settings, false)).toBe(true);
    expect(shouldOfferUpdate({ ...update, available: false }, settings, false)).toBe(false);
  });
  it('건너뛴 버전은 자동으로 알리지 않지만 수동으로는 다시 확인한다', () => {
    const skipped = { ...settings, update_skipped_version: '0.4.0' };
    expect(shouldOfferUpdate(update, skipped, false)).toBe(false);
    expect(shouldOfferUpdate(update, skipped, true)).toBe(true);
    expect(shouldOfferUpdate({ ...update, version: '0.5.0' }, skipped, false)).toBe(true);
  });
  it('자동 확인을 끄더라도 수동 확인은 가능하다', () => {
    const disabled = { ...settings, update_check_disabled: true };
    expect(shouldOfferUpdate(update, disabled, false)).toBe(false);
    expect(shouldOfferUpdate(update, disabled, true)).toBe(true);
  });
  it('확인 실패는 조용히 처리한다', () => {
    expect(shouldOfferUpdate({ ...update, available: false, check_failed: true, version: null }, settings, false)).toBe(false);
  });
});
