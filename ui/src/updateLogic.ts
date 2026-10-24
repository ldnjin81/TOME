import type { Settings } from './types';

export interface UpdateInfo {
  available: boolean;
  current: string;
  version: string | null;
  notes: string | null;
  date: string | null;
  installable: boolean;
  check_failed: boolean;
}

/** 수동 확인은 건너뛴 버전도 다시 보여 준다. */
export function shouldOfferUpdate(info: UpdateInfo, settings: Settings, manual: boolean): boolean {
  return info.available && !!info.version && (manual || (!settings.update_check_disabled && settings.update_skipped_version !== info.version));
}
