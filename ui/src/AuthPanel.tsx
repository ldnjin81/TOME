import { useEffect, useRef, useState } from 'react';
import { Channel, invoke } from '@tauri-apps/api/core';
import { t, getLocale } from './i18n';
import { explainError } from './errors';
import type { AuthState } from './types';

type AuthEvent = { tagName: 'authUrl' | 'authPending' | 'authUserInfo'; data: { url?: string; userCode?: string; remainingSecs?: number; id?: string; name?: string } };

/** 같은 로그인 화면을 서버 설정과 현재 작업본 신원 영역에서 사용한다. */
export default function AuthPanel({ path = '', remoteUrl = '', onState, onChanged }: {
  path?: string; remoteUrl?: string;
  onState?: (state: AuthState) => void;
  onChanged?: () => void;
}) {
  const [state, setState] = useState<AuthState | null>(null);
  const [error, setError] = useState('');
  const [active, setActive] = useState(false);
  const [url, setUrl] = useState('');
  const [code, setCode] = useState('');
  const [deadline, setDeadline] = useState(0);
  const [clock, setClock] = useState(Date.now());
  const [notice, setNotice] = useState('');
  const [copied, setCopied] = useState(false);
  const attempt = useRef('');
  const generation = useRef(0);
  const callbacks = useRef({ onState, onChanged });
  callbacks.current = { onState, onChanged };
  const target = { path, remoteUrl };

  async function refresh(version = generation.current) {
    try {
      const next = await invoke<AuthState>('auth_state', target);
      if (generation.current !== version) return;
      setState(next);
      callbacks.current.onState?.(next);
    } catch (e) {
      if (generation.current === version) setError(String(e));
    }
  }

  useEffect(() => {
    const version = ++generation.current;
    setState(null); setError(''); setUrl(''); setCode(''); setNotice(''); setActive(false); setDeadline(0);
    const timer = setTimeout(() => { if (path || remoteUrl.trim()) void refresh(version); }, 350);
    // 취소된 호출이 백그라운드에서 끝났는지도 새로 확인한다.
    const polling = setInterval(() => { if (path || remoteUrl.trim()) void refresh(version); }, 5000);
    return () => {
      ++generation.current;
      clearTimeout(timer); clearInterval(polling);
      if (attempt.current) void invoke('auth_cancel', { id: attempt.current }).catch(() => {});
      attempt.current = '';
    };
    // 대상이 바뀌면 이전 응답과 이벤트를 무시한다.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [path, remoteUrl]);

  useEffect(() => {
    const timer = setInterval(() => setClock(Date.now()), 1000);
    return () => clearInterval(timer);
  }, []);

  async function login() {
    const id = crypto.randomUUID();
    const version = generation.current;
    attempt.current = id;
    setActive(true); setError(''); setNotice(''); setUrl(''); setCode(''); setDeadline(0); setCopied(false);
    const onEvent = new Channel<AuthEvent>();
    onEvent.onmessage = event => {
      if (attempt.current !== id || generation.current !== version) return;
      if (event.tagName === 'authUrl') { setUrl(event.data.url ?? ''); setCode(event.data.userCode ?? ''); }
      if (event.tagName === 'authPending') setDeadline(Date.now() + (event.data.remainingSecs ?? 0) * 1000);
      if (event.tagName === 'authUserInfo') setNotice(t('로그인 성공: {name}', { name: event.data.name || event.data.id || '' }));
    };
    try {
      await invoke('auth_login', { ...target, id, onEvent });
      if (attempt.current !== id || generation.current !== version) return;
      await refresh(version);
      callbacks.current.onChanged?.();
    } catch (e) {
      if (attempt.current === id && generation.current === version) setError(String(e));
    } finally {
      if (attempt.current === id && generation.current === version) {
        attempt.current = ''; setActive(false); setUrl(''); setCode(''); setDeadline(0);
        void refresh(version);
      }
    }
  }

  async function cancel() {
    const id = attempt.current;
    attempt.current = ''; setActive(false); setUrl(''); setCode(''); setDeadline(0);
    setNotice(t('대기를 취소했습니다. 이미 승인한 로그인은 완료될 수 있습니다. 이전 요청이 끝나면 다시 시도할 수 있습니다.'));
    try { await invoke('auth_cancel', { id }); await refresh(); } catch (e) { setError(String(e)); }
  }

  async function logout() {
    const version = generation.current;
    setActive(true); setError(''); setNotice('');
    try {
      await invoke('auth_logout', target);
      if (generation.current !== version) return;
      await refresh(version);
      callbacks.current.onChanged?.();
    } catch (e) { if (generation.current === version) setError(String(e)); }
    finally { if (generation.current === version) setActive(false); }
  }

  const expired = state?.status === 'expired' || (!!state?.expires && state.expires <= clock);
  return <div className="auth-panel" aria-label={t('서버 로그인')}>
    {state?.status === 'disabled' ? <p className="muted small">{t('인증 없는 서버 · 이름으로만 기록')}</p> : <>
      {state?.status === 'error' && <p className="error-line" role="alert">{explainError(state.detail)}</p>}
      {state?.user && <p className="small">{t('로그인: {p0}', { p0: `${state.user.name} (${state.user.id})` })}</p>}
      {expired && <p className="warn-line">{t('로그인이 필요하거나 만료되었습니다')}</p>}
      {state?.user && !expired && <p className="muted small">{state.expires
        ? t('만료: {time}', { time: new Date(state.expires).toLocaleString(getLocale()) })
        : t('만료 시간 알 수 없음')}</p>}
      {state?.status === 'required' && <p className="muted small">{t('이 서버는 로그인이 필요합니다')}</p>}
      <div className="field-row">
        <button className="ghost small-btn" disabled={active || !!state?.login_in_progress || (!path && !remoteUrl.trim())} onClick={() => void login()}>{t('로그인')}</button>
        {state?.user && <button className="ghost small-btn" disabled={active || !!state.login_in_progress} onClick={() => void logout()}>{t('로그아웃')}</button>}
      </div>
    </>}
    {active && attempt.current && <div className="auth-pending" role="status">
      <p>{t('브라우저에서 로그인을 승인하세요.')}</p>
      {url && <>
        <button className="link" onClick={() => void invoke('auth_open_url', { url }).catch(e => setError(String(e)))}>{t('로그인 URL 열기')}</button>
        <p className="small mono auth-url">{url}</p>
      </>}
      {code && <p><strong className="mono">{code}</strong>{' '}<button className="ghost small-btn" onClick={() => void navigator.clipboard.writeText(code).then(() => setCopied(true), e => setError(String(e)))}>{copied ? t('복사됨') : t('코드 복사')}</button></p>}
      <p>{deadline ? t('승인 대기 · {seconds}초 남음', { seconds: Math.max(0, Math.ceil((deadline - clock) / 1000)) }) : t('준비 중…')}</p>
      <button className="ghost small-btn" onClick={() => void cancel()}>{t('취소')}</button>
    </div>}
    {!active && state?.login_in_progress && <p className="muted small">{t('이전 로그인 요청이 끝나기를 기다리는 중…')}</p>}
    {notice && <p className="small" role="status">{notice}</p>}
    {error && <p className="error-line" role="alert">{explainError(error)}</p>}
  </div>;
}


export function AuthDialog({ path, remoteUrl, onChanged, onClose }: { path: string; remoteUrl: string; onChanged: () => void; onClose: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => { dialog.current?.showModal(); }, []);
  return <dialog ref={dialog} className="setup-dialog" onClose={onClose} aria-label={t('서버 로그인')}>
    <h2>{t('서버 로그인')}</h2>
    <p className="muted small mono auth-url">{path || remoteUrl}</p>
    <AuthPanel path={path} remoteUrl={remoteUrl} onChanged={onChanged} />
    <div className="dialog-buttons"><button className="ghost" onClick={() => dialog.current?.close()}>{t('닫기')}</button></div>
  </dialog>;
}
