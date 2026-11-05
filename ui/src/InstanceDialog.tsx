import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { t } from './i18n';
import { explainError } from './errors';
import { browseFolder } from './SetupDialog';
import type { InstanceReport } from './types';

const PHASES: Record<InstanceReport['phases'][number], string> = {
  deregistering: '작업본 등록 해제',
  unmounting: 'SWFS 마운트 해제',
  deleting_files: '로컬 파일 삭제',
};

/** 같은 폴더 안에서 경로 끝 이름만 바꾼 이동 대상 제안. */
export function suggestTarget(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, '');
  return trimmed ? `${trimmed}-moved` : '';
}

/** 선택한 상위 폴더 아래에 현재 작업본 이름으로 대상을 만든다. */
export function targetUnder(folder: string, path: string): string {
  const name = path.replace(/[\\/]+$/, '').split(/[\\/]/).pop() ?? '';
  const separator = folder.includes('\\') ? '\\' : '/';
  return `${folder.replace(/[\\/]+$/, '')}${separator}${name}`;
}

/**
 * Lore 0.13 `instance move/delete`로 열린 작업본을 옮기거나 지운다.
 * 둘 다 먼저 미리보기(dry run)로 Lore의 사전 검사를 통과해야 실행할 수 있다.
 */
export default function InstanceDialog({ path, changes, drafts, onMoved, onDeleted, onClose }: {
  path: string;
  /** 커밋하지 않은 변경 파일 수. */
  changes: number;
  /** 서버에 올리지 않은 드래프트가 있는지. */
  drafts: boolean;
  onMoved: (target: string) => void;
  onDeleted: () => void;
  onClose: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [mode, setMode] = useState<'move' | 'delete'>('move');
  const [target, setTarget] = useState(suggestTarget(path));
  const [preview, setPreview] = useState<InstanceReport | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');

  useEffect(() => { dialog.current?.showModal(); }, []);
  // 입력이 바뀌면 이전 미리보기는 더 이상 유효하지 않다.
  useEffect(() => { setPreview(null); setConfirmed(false); setError(''); }, [mode, target]);

  async function call(dryRun: boolean) {
    setBusy(true); setError('');
    try {
      const report = mode === 'move'
        ? await invoke<InstanceReport>('instance_move', { path, target, dryRun })
        : await invoke<InstanceReport>('instance_delete', { path, dryRun });
      if (dryRun) setPreview(report);
      else if (mode === 'move') onMoved(report.path);
      else onDeleted();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function pickFolder() {
    try {
      const folder = await browseFolder(t('옮길 위치의 상위 폴더'), path);
      if (folder) setTarget(targetUnder(folder, path));
    } catch (e) { setError(String(e)); }
  }

  const risky = changes > 0 || drafts;
  return <dialog ref={dialog} className="setup-dialog" onClose={onClose} aria-labelledby="instance-title">
    <h2 id="instance-title">{t('작업본 관리')}</h2>
    <p className="muted small mono auth-url">{path}</p>
    <div className="mode-switch" role="radiogroup" aria-label={t('작업')}>
      <button role="radio" aria-checked={mode === 'move'} className={mode === 'move' ? 'active' : ''} onClick={() => setMode('move')} disabled={busy}>{t('이동@@동작')}</button>
      <button role="radio" aria-checked={mode === 'delete'} className={mode === 'delete' ? 'active' : ''} onClick={() => setMode('delete')} disabled={busy}>{t('삭제@@동작')}</button>
    </div>

    {mode === 'move' ? <>
      <p className="small">{t('작업본 폴더를 통째로 옮기고 Lore에 새 위치를 등록합니다. 대상 폴더는 아직 없어야 합니다.')}</p>
      <label className="visually-hidden" htmlFor="instance-target">{t('새 위치')}</label>
      <div className="field-row">
        <input id="instance-target" value={target} onChange={(e) => setTarget(e.target.value)} spellCheck={false} disabled={busy} />
        <button className="ghost" onClick={() => void pickFolder()} disabled={busy}>{t('찾아보기')}</button>
      </div>
      {preview && <p className="small" role="status">{t('이동할 수 있습니다: {target}', { target: preview.path })}</p>}
    </> : <>
      <p className="small">{t('이 컴퓨터의 작업본 폴더와 등록 정보를 지웁니다. 서버의 저장소와 브랜치는 그대로 남습니다.')}</p>
      {risky && <p className="warn-line" role="alert">{[
        changes > 0 ? t('커밋하지 않은 변경 {count}개가 사라집니다.', { count: changes }) : '',
        drafts ? t('서버에 올리지 않은 드래프트가 사라집니다.') : '',
      ].filter(Boolean).join(' ')}</p>}
      {preview && <div role="status">
        <p className="small">{t('Lore가 다음 순서로 진행합니다:')}</p>
        <ol className="small">{preview.phases.map((phase) => <li key={phase}>{t(PHASES[phase] ?? phase)}</li>)}</ol>
        {preview.is_swfs && <p className="muted small">{t('SWFS 작업본이라 먼저 마운트를 해제합니다.')}</p>}
        <label className="check"><input type="checkbox" checked={confirmed} onChange={(e) => setConfirmed(e.target.checked)} disabled={busy} />{t('로컬 파일이 지워지는 것을 확인했습니다')}</label>
      </div>}
    </>}

    {error && <p className="error-line" role="alert">{explainError(error)}</p>}
    <div className="dialog-buttons">
      <button className="ghost" onClick={() => dialog.current?.close()} disabled={busy}>{t('닫기')}</button>
      {!preview
        ? <button onClick={() => void call(true)} disabled={busy || (mode === 'move' && !target.trim())}>{busy ? t('확인 중…') : t('미리보기')}</button>
        : <button className={mode === 'delete' ? 'danger' : ''} onClick={() => void call(false)} disabled={busy || (mode === 'delete' && !confirmed)}>
          {busy ? t('작업 중…') : mode === 'move' ? t('이동@@동작') : t('작업본 삭제')}
        </button>}
    </div>
  </dialog>;
}
