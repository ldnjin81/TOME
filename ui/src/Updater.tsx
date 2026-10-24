import { useEffect, useRef, useState } from 'react';
import { getVersion } from '@tauri-apps/api/app';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { t } from './i18n';
import type { Settings } from './types';
import { shouldOfferUpdate, type UpdateInfo } from './updateLogic';

interface Props {
  settings: Settings;
  saveSettings: (settings: Settings) => Promise<void>;
  runningJob: boolean;
  settingsOpen: boolean;
  onCloseSettings: () => void;
}
interface Progress { received: number; total: number | null; phase: string }

export default function Updater({ settings, saveSettings, runningJob, settingsOpen, onCloseSettings }: Props) {
  const [currentVersion, setCurrentVersion] = useState('…');
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [offered, setOffered] = useState(false);
  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [message, setMessage] = useState('');
  const [error, setError] = useState('');
  const [notes, setNotes] = useState(false);
  const [confirm, setConfirm] = useState(false);
  const dialog = useRef<HTMLDialogElement>(null);
  const currentSettings = useRef(settings);
  currentSettings.current = settings;
  const inFlight = useRef(false);

  async function check(manual: boolean) {
    if (inFlight.current || installing) return;
    inFlight.current = true;
    setChecking(true); setError(''); setMessage('');
    try {
      const result = await invoke<UpdateInfo>('update_check');
      setInfo(result);
      setOffered(shouldOfferUpdate(result, currentSettings.current, manual));
      if (manual && !result.available) setMessage(result.check_failed ? t('업데이트를 확인하지 못했습니다. 연결 후 다시 시도하세요.') : t('최신 버전입니다.'));
    } catch {
      if (manual) setMessage(t('업데이트를 확인하지 못했습니다. 연결 후 다시 시도하세요.'));
    } finally { inFlight.current = false; setChecking(false); }
  }

  useEffect(() => {
    void getVersion().then(setCurrentVersion, () => {});
    // 설정을 불러온 뒤 한 번만 예약하고, 실행 직전에 최신 설정을 확인한다.
    const timer = setTimeout(() => { if (!currentSettings.current.update_check_disabled) void check(false); }, 3500);
    return () => clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const stop = listen<Progress>('tome-update-progress', e => setProgress(e.payload));
    return () => { void stop.then(fn => fn()); };
  }, []);

  useEffect(() => {
    if (settingsOpen) dialog.current?.showModal();
    else dialog.current?.close();
  }, [settingsOpen]);

  async function openRelease() {
    try { await invoke('update_open_release'); }
    catch { setError(t('릴리스 페이지를 열지 못했습니다. 브라우저에서 GitHub TOME 릴리스를 확인하세요.')); }
  }

  async function install(allowRunningJobs = false) {
    if (!info || installing) return;
    if (!info.installable) { await openRelease(); return; }
    if (runningJob && !allowRunningJobs) { setConfirm(true); return; }
    setConfirm(false); setInstalling(true); setError(''); setProgress(null);
    try {
      await invoke('update_install', { version: info.version, allowRunningJobs });
    } catch (e) {
      setInstalling(false);
      if (String(e) === 'jobs_running') { setConfirm(true); return; }
      setError(String(e) === 'check_again' ? t('새 버전을 다시 확인한 뒤 업데이트하세요.') : t('업데이트 설치에 실패했습니다. 다시 시도하거나 릴리스 페이지에서 설치 파일을 받으세요.'));
    }
  }

  const banner = offered && info && <section className="update-banner" aria-label={t('TOME 업데이트')} aria-live="polite">
    <strong>{t('TOME {version} 사용 가능', { version: info.version })}</strong>
    {info.date && <small className="muted">{info.date.slice(0, 10)}</small>}
    {!info.installable && <p>{t('포터블판·맥·개발판은 릴리스 페이지에서 업데이트하세요.')}</p>}
    {installing ? <div role="status">
      <p>{progress?.phase === 'install' ? t('서명 확인 후 설치·재시작하는 중…') : t('업데이트 다운로드 중…')}</p>
      <progress max={progress?.total || undefined} value={progress?.total ? progress.received : undefined} />
      <small>{progress ? `${progress.received.toLocaleString()} / ${progress.total?.toLocaleString() ?? '?'} bytes` : ''}</small>
    </div> : <div className="update-actions">
      <button className="primary" onClick={() => void install()}>{info.installable ? t('업데이트') : t('릴리스 페이지 열기')}</button>
      <button className="ghost" onClick={() => setNotes(!notes)} aria-expanded={notes}>{t('변경 내용')}</button>
      <button className="ghost" onClick={() => setOffered(false)}>{t('나중에')}</button>
      <button className="ghost" onClick={() => { void saveSettings({ ...currentSettings.current, update_skipped_version: info.version ?? '' }); setOffered(false); }}>{t('이 버전 건너뛰기')}</button>
    </div>}
    {notes && <pre className="update-notes">{info.notes || t('변경 내용이 없습니다.')}</pre>}
    {confirm && <div role="alert">
      <p>{t('진행 중인 작업이 있습니다. 설치하면 앱이 종료됩니다. 계속할까요?')}</p>
      <button className="primary" onClick={() => void install(true)}>{t('설치·재시작')}</button>
      <button className="ghost" onClick={() => setConfirm(false)}>{t('취소')}</button>
    </div>}
    {error && <p role="alert">{error}</p>}
  </section>;

  return <>
    {!settingsOpen && banner && <div className="update-toast">{banner}</div>}
    {!settingsOpen && error && !banner && <div className="update-toast" role="alert">{error}</div>}
    <dialog ref={dialog} className="setup-dialog update-dialog" onCancel={onCloseSettings} onClose={onCloseSettings} aria-labelledby="update-title">
      <h2 id="update-title">{t('TOME 업데이트')}</h2>
      <p>{t('현재 버전: {version}', { version: info?.current ?? currentVersion })}</p>
      <label className="check"><input type="checkbox" checked={!settings.update_check_disabled} onChange={e => void saveSettings({ ...currentSettings.current, update_check_disabled: !e.target.checked })} />{t('시작할 때 업데이트 확인')}</label>
      <div className="update-actions"><button className="ghost" onClick={() => void check(true)} disabled={checking || installing}>{checking ? t('확인 중…') : t('지금 확인')}</button><button className="ghost" onClick={() => void openRelease()}>{t('릴리스 페이지 열기')}</button></div>
      {message && <p role="status">{message}</p>}
      {banner}
      {error && !banner && <p role="alert">{error}</p>}
      <div className="dialog-buttons"><button className="ghost" onClick={onCloseSettings}>{t('닫기')}</button></div>
    </dialog>
  </>;
}
