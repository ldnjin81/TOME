import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open as pickFolder } from '@tauri-apps/plugin-dialog';
import type { Done, RemoteRepository, Settings } from './types';
import { PRESETS } from './ViewDialog';

const DEFAULT_SERVER = 'lore://lore.example.com:41337';

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
  onCancel: () => void;
}

/** First-run setup (and ⚙ later): the Lore server, then a working copy to open or to clone. */
export default function SetupDialog({ settings, firstRun, onDone, onCancel }: Props) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [server, setServer] = useState(settings.server || DEFAULT_SERVER);
  const [offline, setOffline] = useState(settings.offline);
  const [identity, setIdentity] = useState(settings.identity ?? '');
  const [source, setSource] = useState<'open' | 'clone'>('open');
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
    setBusy('서버에 연결하는 중…');
    setError('');
    try {
      const done = await invoke<Done<RemoteRepository[]>>('list_repositories', { server: server.trim() });
      setRepositories(done.value);
      if (!repository && done.value[0]) choose(done.value[0].name);
    } catch (e) {
      setRepositories(null);
      setError(`연결 실패: ${e}`);
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
    const next: Settings = { ...settings, server: server.trim(), offline, identity: identity.trim(), setup_done: true };
    // Clone and the repository list already run as this identity.
    await invoke('save_settings', { settings: next }).catch(() => {});
    if (source === 'open') {
      onDone(next, openPath.trim() || null);
      return;
    }
    setBusy('저장소를 받는 중… 큰 저장소는 오래 걸립니다');
    setError('');
    try {
      const view = PRESETS.find((p) => p.name === preset)?.lines.join('\n') ?? '';
      await invoke<Done<null>>('clone_repository', { path: target, url: `${server.trim().replace(/\/$/, '')}/${repository}`, view });
      onDone(next, target);
    } catch (e) {
      setError(`받기 실패: ${e}`);
    } finally {
      setBusy('');
    }
  }

  return (
    <dialog
      ref={dialog}
      className="setup-dialog"
      aria-labelledby="setup-title"
      onCancel={(e) => {
        if (busy) e.preventDefault();
      }}
      onClose={onCancel}
    >
      <h2 id="setup-title">{firstRun ? 'TOME 시작하기' : '설정'}</h2>

      <section className="setup-step">
        <h3>
          <span className="step-no">1</span> Lore 서버
        </h3>
        <div className="field-row">
          <label className="visually-hidden" htmlFor="setup-server">서버 주소</label>
          <input id="setup-server" className="mono" value={server} onChange={(e) => setServer(e.target.value)} spellCheck={false} placeholder={DEFAULT_SERVER} />
          <button className="ghost" onClick={() => void connect()} disabled={!!busy || !server.trim()}>
            연결 확인
          </button>
        </div>
        {repositories && <p className="ok-line">연결됨 · 저장소 {repositories.length}개</p>}
        <label className="field">
          <span>내 이름(신원)</span>
          <input id="setup-identity" value={identity} onChange={(e) => setIdentity(e.target.value)} placeholder="예: kim-pc" spellCheck={false} />
        </label>
        <p className="muted hint">인증 없는 서버에서는 이 이름이 커밋 작성자로 기록됩니다. PC마다 다르게 쓰면(kim-pc, kim-laptop) 어디서 커밋했는지 구분됩니다.</p>
        <label className="check">
          <input type="checkbox" checked={offline} onChange={(e) => setOffline(e.target.checked)} />
          열 때 오프라인으로 읽기(서버 없이 로컬 데이터만)
        </label>
      </section>

      <section className="setup-step">
        <h3>
          <span className="step-no">2</span> 작업본
        </h3>
        <div className="segmented" role="radiogroup" aria-label="작업본">
          <label className={source === 'open' ? 'active' : ''}>
            <input type="radio" name="source" checked={source === 'open'} onChange={() => setSource('open')} />
            이미 있는 작업본 열기
          </label>
          <label className={source === 'clone' ? 'active' : ''}>
            <input type="radio" name="source" checked={source === 'clone'} onChange={() => setSource('clone')} />
            서버에서 받기
          </label>
        </div>

        {source === 'open' ? (
          <div className="field-row">
            <label className="visually-hidden" htmlFor="setup-open">작업본 폴더</label>
            <input id="setup-open" className="mono" value={openPath} onChange={(e) => setOpenPath(e.target.value)} spellCheck={false} placeholder="C:\Project\SampleProject" />
            <button className="ghost" onClick={() => void browseFolder('작업본 폴더', openPath).then((p) => p && setOpenPath(p))} disabled={!!busy}>
              찾아보기
            </button>
          </div>
        ) : (
          <div className="clone-fields">
            {repositories === null ? (
              <p className="muted">먼저 서버에 연결하면 저장소 목록이 나옵니다.</p>
            ) : repositories.length === 0 ? (
              <p className="muted">서버에 저장소가 없습니다.</p>
            ) : (
              <ul className="repo-list" role="listbox" aria-label="저장소">
                {repositories.map((r) => (
                  <li key={r.id || r.name}>
                    <button role="option" aria-selected={r.name === repository} className={r.name === repository ? 'repo active' : 'repo'} onClick={() => choose(r.name)}>
                      {r.name}
                    </button>
                  </li>
                ))}
              </ul>
            )}
            <label htmlFor="setup-parent">받을 위치</label>
            <div className="field-row">
              <input id="setup-parent" className="mono" value={parent} onChange={(e) => setParent(e.target.value)} spellCheck={false} placeholder="C:\Project" />
              <button className="ghost" onClick={() => void browseFolder('받을 위치', parent).then((p) => p && setParent(p))} disabled={!!busy}>
                찾아보기
              </button>
            </div>
            <label htmlFor="setup-folder">폴더 이름</label>
            <input id="setup-folder" className="mono" value={folder} onChange={(e) => setFolder(e.target.value)} spellCheck={false} />
            <label>받을 범위(View)</label>
            <div className="presets" role="group" aria-label="받을 범위">
              {PRESETS.map((p) => (
                <button key={p.name} className={p.name === preset ? 'chip active' : 'chip'} onClick={() => setPreset(p.name)}>
                  {p.name}
                </button>
              ))}
            </div>
            {target && <p className="muted mono target">→ {target}</p>}
          </div>
        )}
      </section>

      {busy && <p className="busy-line" role="status">{busy}</p>}
      {error && <p className="error-line" role="alert">{error}</p>}

      <div className="dialog-buttons">
        <button className="ghost" onClick={() => dialog.current?.close()} disabled={!!busy}>
          {firstRun ? '나중에' : '닫기'}
        </button>
        <button className="primary" onClick={() => void finish()} disabled={!canFinish}>
          {source === 'clone' ? '받고 열기' : firstRun ? '시작' : '저장'}
        </button>
      </div>
    </dialog>
  );
}
