import { t, setLanguage, type LanguageSetting } from './i18n';
import { useEffect, useRef, useState } from 'react';
import { explainError } from './errors';
import { invoke } from '@tauri-apps/api/core';
import { open as pickFolder } from '@tauri-apps/plugin-dialog';
import type { Done, RemoteRepository, Settings } from './types';
import AuthPanel from './AuthPanel';
import { PRESETS } from './ViewDialog';

/** Lore's default port; the host is the team's server. */
const SERVER_EXAMPLE = 'lore://lore.example.com:41337';

/** Opens the system folder picker; null when cancelled. */
export async function browseFolder(title: string, start?: string): Promise<string | null> {
  const picked = await pickFolder({ directory: true, multiple: false, title, defaultPath: start || undefined });
  return typeof picked === 'string' ? picked : null;
}

function joinPath(parent: string, name: string) {
  const sep = parent.includes('\\') ? '\\' : '/';
  return parent.endsWith(sep) ? parent + name : parent + sep + name;
}

interface Props {
  settings: Settings;
  firstRun: boolean;
  /** Called with the new settings and the working copy to open (null: keep the current one). */
  onDone: (settings: Settings, open: string | null) => void;
  /** Clone `url` into `target` (a job with progress), then open it. */
  onClone: (settings: Settings, target: string, url: string, view: string, offlineReady: boolean) => void;
  onCancel: () => void;
}

/** First-run setup (and ⚙ later): the Lore server, then a working copy to open or to clone. */
export default function SetupDialog({ settings, firstRun, onDone, onClone, onCancel }: Props) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [server, setServer] = useState(settings.server);
  const [offline, setOffline] = useState(settings.offline);
  const [identity, setIdentity] = useState(settings.identity ?? '');
  const [language, setDialogLanguage] = useState<LanguageSetting>(settings.language ?? '');
  const [source, setSource] = useState<'open' | 'clone'>('open');
  const [offlineReady, setOfflineReady] = useState(false);
  const [openPath, setOpenPath] = useState(settings.recent[0] ?? '');
  const [repositories, setRepositories] = useState<RemoteRepository[] | null>(null);
  const [repository, setRepository] = useState('');
  const [parent, setParent] = useState('');
  const [folder, setFolder] = useState('');
  const [preset, setPreset] = useState(PRESETS[0].name);
  const [busy, setBusy] = useState('');
  const [error, setError] = useState('');

  useEffect(() => {
    dialog.current?.showModal();
    if (!settings.identity) invoke<string>('default_identity').then((name) => setIdentity((now) => now || name), () => {});
  }, [settings.identity]);

  async function connect() {
    setBusy(t("서버에 연결하는 중…"));
    setError('');
    try {
      const done = await invoke<Done<RemoteRepository[]>>('list_repositories', { server: server.trim() });
      setRepositories(done.value);
      if (!repository && done.value[0]) choose(done.value[0].name);
    } catch (e) {
      setRepositories(null);
      setError(t("연결 실패: {p0}", { p0: String(e) }));
    } finally {
      setBusy('');
    }
  }

  function choose(name: string) {
    setRepository(name);
    setFolder(name);
  }

  const target = parent && folder.trim() ? joinPath(parent, folder.trim()) : '';
  const canFinish = !busy && (source === 'open' ? openPath.trim() !== '' || !firstRun : repository !== '' && target !== '');

  async function finish() {
    const next: Settings = { ...settings, server: server.trim(), offline, identity: identity.trim(), language, setup_done: true };
    // Clone and the repository list already run as this identity.
    await invoke('save_settings', { settings: next }).catch(() => {});
    if (source === 'open') {
      onDone(next, openPath.trim() || null);
      return;
    }
    const view = PRESETS.find((p) => p.name === preset)?.lines.join('\n') ?? '';
    onClone(next, target, `${server.trim().replace(/\/$/, '')}/${repository}`, view, offlineReady);
  }

  return (
    <dialog
      ref={dialog}
      className="setup-dialog"
      aria-labelledby="setup-title"
      onCancel={(e) => {
        if (busy) e.preventDefault();
      }}
      onClose={() => { setLanguage(settings.language ?? ''); onCancel(); }}
    >
      <h2 id="setup-title">{firstRun ? t("TOME 시작하기") : t("설정")}</h2>

      <label className="field">
        <span>{t('언어')}</span>
        <select value={language} onChange={(event) => {
          const value = event.target.value as LanguageSetting;
          setDialogLanguage(value);
          setLanguage(value);
        }}>
          <option value="">{t('자동 (시스템 언어)')}</option>
          <option value="ko">{t('한국어')}</option>
          <option value="en">English</option>
        </select>
      </label>

      <section className="setup-step">
        <h3>
          <span className="step-no">1</span> {t("Lore 서버")}
        </h3>
        <div className="field-row">
          <label className="visually-hidden" htmlFor="setup-server">{t("서버 주소")}</label>
          <input id="setup-server" className="mono" value={server} onChange={(e) => setServer(e.target.value)} spellCheck={false} placeholder={SERVER_EXAMPLE} />
          <button className="ghost" onClick={() => void connect()} disabled={!!busy || !server.trim()}>
            {t("연결 확인")}
          </button>
        </div>
        <AuthPanel remoteUrl={server.trim()} onChanged={() => { setRepositories(null); void connect(); }} />
        {repositories && <p className="ok-line">{t("연결됨 · 저장소 {p0}개", { p0: repositories.length })}</p>}
        <label className="field">
          <span>{t("내 이름(신원)")}</span>
          <input id="setup-identity" value={identity} onChange={(e) => setIdentity(e.target.value)} placeholder={t("예: kim-pc")} spellCheck={false} />
        </label>
        <p className="muted hint">{t("인증 없는 서버에서는 이 이름이 커밋 작성자로 기록됩니다. PC마다 다르게 쓰면(kim-pc, kim-laptop) 어디서 커밋했는지 구분됩니다.")}</p>
        <label className="check">
          <input type="checkbox" checked={offline} onChange={(e) => setOffline(e.target.checked)} />
          {t("열 때 오프라인으로 읽기(서버 없이 로컬 데이터만)")}
        </label>
      </section>

      <section className="setup-step">
        <h3>
          <span className="step-no">2</span> {t("작업본")}
        </h3>
        <div className="segmented" role="radiogroup" aria-label={t("작업본")}>
          <label className={source === 'open' ? 'active' : ''}>
            <input type="radio" name="source" checked={source === 'open'} onChange={() => setSource('open')} />
            {t("이미 있는 작업본 열기")}
          </label>
          <label className={source === 'clone' ? 'active' : ''}>
            <input type="radio" name="source" checked={source === 'clone'} onChange={() => setSource('clone')} />
            {t("서버에서 받기")}
          </label>
        </div>

        {source === 'open' ? (
          <div className="field-row">
            <label className="visually-hidden" htmlFor="setup-open">{t("작업본 폴더")}</label>
            <input id="setup-open" className="mono" value={openPath} onChange={(e) => setOpenPath(e.target.value)} spellCheck={false} placeholder="C:\Project\MyGame" />
            <button className="ghost" onClick={() => void browseFolder(t("작업본 폴더"), openPath).then((p) => p && setOpenPath(p))} disabled={!!busy}>
              {t("찾아보기")}
            </button>
          </div>
        ) : (
          <div className="clone-fields">
            {repositories === null ? (
              <p className="muted">{t("먼저 서버에 연결하면 저장소 목록이 나옵니다.")}</p>
            ) : repositories.length === 0 ? (
              <p className="muted">{t("서버에 저장소가 없습니다.")}</p>
            ) : (
              <ul className="repo-list" role="listbox" aria-label={t("저장소")}>
                {repositories.map((r) => (
                  <li key={r.id || r.name}>
                    <button role="option" aria-selected={r.name === repository} className={r.name === repository ? 'repo active' : 'repo'} onClick={() => choose(r.name)}>
                      {r.name}
                    </button>
                  </li>
                ))}
              </ul>
            )}
            <label htmlFor="setup-parent">{t("받을 위치")}</label>
            <div className="field-row">
              <input id="setup-parent" className="mono" value={parent} onChange={(e) => setParent(e.target.value)} spellCheck={false} placeholder="C:\Project" />
              <button className="ghost" onClick={() => void browseFolder(t("받을 위치"), parent).then((p) => p && setParent(p))} disabled={!!busy}>
                {t("찾아보기")}
              </button>
            </div>
            <label htmlFor="setup-folder">{t("폴더 이름")}</label>
            <input id="setup-folder" className="mono" value={folder} onChange={(e) => setFolder(e.target.value)} spellCheck={false} />
            <label>{t("받을 범위(View)")}</label>
            <div className="presets" role="group" aria-label={t("받을 범위")}>
              {PRESETS.map((p) => (
                <button key={p.name} className={p.name === preset ? 'chip active' : 'chip'} onClick={() => setPreset(p.name)}>
                  {t(p.name)}
                </button>
              ))}
            </div>
            <label className="check" title={t("받은 뒤 서버 없이도 스테이징·커밋할 수 있게 필요한 데이터를 미리 받아 둡니다.")}>
              <input type="checkbox" checked={offlineReady} onChange={(e) => setOfflineReady(e.target.checked)} />
              {t("오프라인 작업 준비")}
            </label>
            {target && <p className="muted mono target">→ {target}</p>}
          </div>
        )}
      </section>

      {busy && <p className="busy-line" role="status">{busy}</p>}
      {error && <p className="error-line" role="alert">{explainError(error)}</p>}

      <div className="dialog-buttons">
        <button className="ghost" onClick={() => dialog.current?.close()} disabled={!!busy}>
          {firstRun ? t("나중에") : t("닫기")}
        </button>
        <button className="primary" onClick={() => void finish()} disabled={!canFinish}>
          {source === 'clone' ? t("받고 열기") : firstRun ? t("시작") : t("저장")}
        </button>
      </div>
    </dialog>
  );
}
