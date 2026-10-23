import { t, getLocale } from './i18n';
import { useEffect, useRef, useState } from 'react';
import { explainError } from './errors';
import { invoke } from '@tauri-apps/api/core';
import type { Done, FileAudit, FilePatch, FileRevision } from './types';
import { PatchView, countLines } from './DiffView';

function formatSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function formatTime(ms: number) {
  return ms ? new Date(ms).toLocaleString(getLocale(), { year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' }) : '';
}

/** The revisions that changed one file, and the selected revision's change to it. */
export default function FileHistory({ path, file, offline, me, onClose, onCommands }: { path: string; file: string; offline: boolean; me: string; onClose: () => void; onCommands: (c: string[]) => void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [history, setHistory] = useState<FileRevision[] | null>(null);
  const [error, setError] = useState('');
  const [selected, setSelected] = useState<string | null>(null);
  const [patches, setPatches] = useState<Record<string, FilePatch | null | 'loading'>>({});
  const [view, setView] = useState<'change' | 'audit'>('change');
  const [audit, setAudit] = useState<FileAudit | null | 'loading'>(null);
  const [auditError, setAuditError] = useState('');

  const showAudit = () => {
    setView('audit');
    if (audit !== null) return;
    setAudit('loading');
    invoke<Done<FileAudit>>('file_audit', { path, file, offline }).then(
      (done) => {
        setAudit(done.value);
        onCommands(done.commands);
      },
      (e) => {
        setAudit(null);
        setAuditError(String(e));
      },
    );
  };

  useEffect(() => {
    dialog.current?.showModal();
    invoke<Done<FileRevision[]>>('file_history', { path, file, offline }).then(
      (done) => {
        setHistory(done.value);
        setSelected(done.value[0]?.revision.id ?? null);
        onCommands(done.commands);
      },
      (e) => setError(String(e)),
    );
    // Loaded once per dialog.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const entry = history?.find((h) => h.revision.id === selected) ?? null;
  useEffect(() => {
    if (!entry || patches[entry.revision.id] !== undefined) return;
    const id = entry.revision.id;
    setPatches((p) => ({ ...p, [id]: 'loading' }));
    invoke<FilePatch | null>('file_patch', { path, file: entry.path, revision: id, parent: entry.revision.parents[0] ?? '', offline }).then(
      (patch) => setPatches((p) => ({ ...p, [id]: patch })),
      () => setPatches((p) => ({ ...p, [id]: null })),
    );
  }, [entry, patches, path, offline]);

  const patch = entry ? patches[entry.revision.id] : undefined;
  return (
    <dialog ref={dialog} className="diff-dialog" onClose={onClose} aria-labelledby="fh-title">
      <header className="diff-head">
        <h2 id="fh-title">
          {t("파일 기록")} <span className="mono muted">{file}</span>
        </h2>
        <button className="ghost" onClick={() => dialog.current?.close()}>
          {t("닫기")}
        </button>
      </header>
      {error && <p className="error-line">{explainError(error)}</p>}
      {!history && !error && <p className="muted">{t("불러오는 중…")}</p>}
      {history && history.length === 0 && <p className="muted">{t("이 파일을 바꾼 리비전이 없습니다(아직 커밋되지 않은 새 파일).")}</p>}
      {history && history.length > 0 && (
        <div className="diff-body">
          <ul className="diff-files fh-list">
            {history.map((h) => {
              const r = h.revision;
              const p = patches[r.id];
              const n = p && p !== 'loading' && !p.binary ? countLines(p.patch) : null;
              return (
                <li key={r.id}>
                  <button className={r.id === selected ? 'df fh active' : 'df fh'} onClick={() => setSelected(r.id)}>
                    <span className="fh-top">
                      <span className="rev-no">r{r.number}</span>
                      <span className="fh-msg">{r.message || t("(메시지 없음)")}</span>
                    </span>
                    <span className="fh-meta muted">
                      {r.author === me && me ? t("나") : r.author} · {formatTime(r.timestamp)} · {formatSize(h.size)}
                      {h.path !== file && t(" · 당시 경로 {p0}", { p0: h.path })}
                      {n && (
                        <>
                          {' '}
                          · <span className="plus">+{n.add}</span> <span className="minus">−{n.del}</span>
                        </>
                      )}
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
          <div className="diff-pane">
            <div className="tabs fh-tabs" role="tablist">
              <button role="tab" aria-selected={view === 'change'} className={view === 'change' ? 'tab active' : 'tab'} onClick={() => setView('change')}>
                {t("변경 내용")}
              </button>
              <button role="tab" aria-selected={view === 'audit'} className={view === 'audit' ? 'tab active' : 'tab'} onClick={showAudit}>
                {t("줄별 보기")}
              </button>
            </div>
            {view === 'audit' && (
              <AuditView
                audit={audit}
                error={auditError}
                history={history}
                selected={selected}
                me={me}
                onPick={(id) => {
                  setSelected(id);
                  setView('change');
                }}
              />
            )}
            {view === 'change' && entry && (
              <p className="mono diff-path">
                r{entry.revision.number} · {entry.path}
              </p>
            )}
            {view === 'change' && patch === 'loading' && <p className="muted diff-note">{t("불러오는 중…")}</p>}
            {view === 'change' && patch === null && entry && !entry.revision.parents[0] && <p className="muted diff-note">{t("첫 리비전이라 비교할 이전 내용이 없습니다(이 리비전에서 추가됨).")}</p>}
            {view === 'change' && patch === null && entry && entry.revision.parents[0] && <p className="muted diff-note">{t("이 리비전의 변경 내용을 불러오지 못했습니다.")}</p>}
            {view === 'change' && patch && patch !== 'loading' && <PatchView patch={patch} />}
          </div>
        </div>
      )}
    </dialog>
  );
}

/** Each line of the file with the revision that last changed it, one label per range. */
function AuditView({ audit, error, history, selected, me, onPick }: { audit: FileAudit | null | 'loading'; error: string; history: FileRevision[]; selected: string | null; me: string; onPick: (id: string) => void }) {
  if (error) return <p className="error-line">{explainError(error)}</p>;
  if (audit === null || audit === 'loading') return <p className="muted diff-note">{t("불러오는 중…")}</p>;
  const known = new Map(history.map((h) => [h.revision.id, h.revision]));
  const rows = audit.lines.map((text, i) => {
    const line = i + 1;
    const index = audit.ranges.findIndex((r) => r.first <= line && line <= r.last);
    return { line, text, index, range: audit.ranges[index] };
  });
  const label = (range: (typeof audit.ranges)[number] | undefined) => {
    if (!range) return '';
    if (range.outcome === 'uncommitted') return t("커밋 안 됨");
    if (range.outcome !== 'attributed') return t("확인 못 함");
    const r = known.get(range.revision);
    if (!r) return range.revision.slice(0, 8);
    return `r${r.number} · ${r.author === me && me ? t("나") : r.author} · ${r.message || t("(메시지 없음)")}`;
  };
  return (
    <>
      <p className="muted diff-note">{t("줄마다 마지막으로 바꾼 리비전을 보여 줍니다. 디스크에서 고친 줄은 '커밋 안 됨'으로 표시합니다.")}</p>
      <div className="audit">
        <table>
          <tbody>
            {rows.map(({ line, text, index, range }) => {
              const start = !!range && range.first === line;
              const classes = [start ? 'start' : '', index % 2 ? 'odd' : '', range && range.revision && range.revision === selected ? 'current' : ''].filter(Boolean).join(' ');
              return (
                <tr key={line} className={classes || undefined}>
                  <td className="who">
                    {start && range && range.outcome === 'attributed' && known.has(range.revision) ? (
                      <button title={t("이 줄들을 마지막으로 바꾼 리비전의 변경 내용 보기")} onClick={() => onPick(range.revision)}>
                        {label(range)}
                      </button>
                    ) : start ? (
                      <button className={range?.outcome === 'uncommitted' ? 'pending' : undefined} disabled>
                        {label(range)}
                      </button>
                    ) : null}
                  </td>
                  <td className="no">{line}</td>
                  <td>{text}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </>
  );
}
