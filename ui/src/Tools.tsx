import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open as pickFile } from '@tauri-apps/plugin-dialog';
import type { RunMode, Tool, ToolContext, ToolOutput, ToolSelection, ToolSet } from './types';

/** A tool as offered in a menu: mine or the project's (which runs only when trusted). */
export interface ToolEntry {
  tool: Tool;
  project: boolean;
  disabled: boolean;
}

export function toolsFor(set: ToolSet | null, context: ToolContext): ToolEntry[] {
  if (!set) return [];
  return [
    ...set.project.filter((t) => t.contexts.includes(context)).map((tool) => ({ tool, project: true, disabled: !set.project_trusted })),
    ...set.personal.filter((t) => t.contexts.includes(context)).map((tool) => ({ tool, project: false, disabled: false })),
  ];
}

const CONTEXT_LABEL: Record<ToolContext, string> = { repository: '작업본(도구 메뉴)', file: '파일 우클릭', revision: '리비전 우클릭' };
const RUN_LABEL: Record<RunMode, string> = { capture: '실행하고 출력 보기', terminal: '새 콘솔 창에서', detached: '띄우고 기다리지 않음' };

export const VARIABLES: [string, string][] = [
  ['%f', '선택한 파일의 전체 경로(파일마다 인자 하나)'],
  ['%F', '선택한 파일의 저장소 기준 경로'],
  ['%n', '선택한 파일 이름'],
  ['%r', '선택한 리비전 ID'],
  ['%R', '선택한 리비전 번호'],
  ['%a', '실행 전에 물어본 입력'],
  ['$r', '작업본 루트 폴더'],
  ['$b', '현재 브랜치'],
  ['%%  $$', '글자 % 또는 $'],
];

function MenuItems({ entries, onRun, onClose }: { entries: ToolEntry[]; onRun: (e: ToolEntry) => void; onClose: () => void }) {
  if (entries.length === 0) return <p className="menu-empty">이 위치에 등록된 도구가 없습니다</p>;
  return (
    <>
      {entries.map((e) => (
        <button
          key={(e.project ? 'p:' : 'm:') + e.tool.id}
          role="menuitem"
          className="menu-item"
          disabled={e.disabled}
          title={e.disabled ? '프로젝트 도구를 신뢰해야 실행됩니다' : `${e.tool.program} ${e.tool.args}`}
          onClick={() => {
            onClose();
            onRun(e);
          }}
        >
          <span>{e.tool.name || '(이름 없음)'}</span>
          {e.project && <span className="menu-tag">{e.disabled ? '프로젝트 · 신뢰 필요' : '프로젝트'}</span>}
        </button>
      ))}
    </>
  );
}

/** The toolbar's 도구 menu: working-copy tools and the manager. */
export function ToolMenu({ set, disabled, onRun, onManage }: { set: ToolSet | null; disabled: boolean; onRun: (e: ToolEntry) => void; onManage: () => void }) {
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const close = (ev: MouseEvent) => {
      if (!box.current?.contains(ev.target as Node)) setOpen(false);
    };
    window.addEventListener('mousedown', close);
    return () => window.removeEventListener('mousedown', close);
  }, [open]);
  return (
    <div className="menu-anchor" ref={box}>
      <button className="ghost" aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen(!open)} disabled={disabled}>
        도구 ▾
      </button>
      {open && (
        <div className="menu" role="menu">
          <MenuItems entries={toolsFor(set, 'repository')} onRun={onRun} onClose={() => setOpen(false)} />
          <div className="menu-sep" />
          <button
            role="menuitem"
            className="menu-item"
            onClick={() => {
              setOpen(false);
              onManage();
            }}
          >
            도구 관리…
          </button>
        </div>
      )}
    </div>
  );
}

/** A right-click menu at the pointer. */
export function ContextMenu({ x, y, title, entries, onRun, onClose, actions = [] }: { x: number; y: number; title: string; entries: ToolEntry[]; onRun: (e: ToolEntry) => void; onClose: () => void; actions?: { label: string; run: () => void }[] }) {
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const close = (ev: MouseEvent) => {
      if (!box.current?.contains(ev.target as Node)) onClose();
    };
    const key = (ev: KeyboardEvent) => {
      if (ev.key === 'Escape') onClose();
    };
    window.addEventListener('mousedown', close);
    window.addEventListener('keydown', key);
    return () => {
      window.removeEventListener('mousedown', close);
      window.removeEventListener('keydown', key);
    };
  }, [onClose]);
  // Keep the menu inside the window.
  const left = Math.min(x, window.innerWidth - 280);
  const top = Math.min(y, window.innerHeight - 60 - entries.length * 34);
  return (
    <div className="menu floating" role="menu" ref={box} style={{ left, top }}>
      <p className="menu-title">{title}</p>
      {actions.map((a) => (
        <button
          key={a.label}
          role="menuitem"
          className="menu-item"
          onClick={() => {
            onClose();
            a.run();
          }}
        >
          {a.label}
        </button>
      ))}
      {actions.length > 0 && <div className="menu-sep" />}
      <MenuItems entries={entries} onRun={onRun} onClose={onClose} />
    </div>
  );
}

/** Before running: the prompt's answer and/or a confirmation, with the command line it will run. */
export function RunDialog({ path, entry, selection, onRun, onCancel }: { path: string; entry: ToolEntry; selection: ToolSelection; onRun: (answer: string) => void; onCancel: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [answer, setAnswer] = useState('');
  const [preview, setPreview] = useState('');
  useEffect(() => {
    dialog.current?.showModal();
  }, []);
  useEffect(() => {
    invoke<string>('preview_tool', { path, project: entry.project, id: entry.tool.id, selection: { ...selection, answer } }).then(setPreview, (e) => setPreview(`⚠ ${e}`));
  }, [path, entry, selection, answer]);
  return (
    <dialog ref={dialog} className="run-dialog" onClose={onCancel} aria-labelledby="run-title">
      <form
        method="dialog"
        onSubmit={(e) => {
          e.preventDefault();
          onRun(answer);
        }}
      >
        <h2 id="run-title">{entry.tool.name}</h2>
        {entry.tool.prompt && (
          <label className="field">
            <span>{entry.tool.prompt}</span>
            <input autoFocus value={answer} onChange={(e) => setAnswer(e.target.value)} />
          </label>
        )}
        <p className="muted">실행할 명령</p>
        <pre className="command-preview">{preview}</pre>
        <div className="dialog-buttons">
          <button type="button" className="ghost" onClick={() => dialog.current?.close()}>
            취소
          </button>
          <button type="submit" className="primary" disabled={preview.startsWith('⚠')}>
            실행
          </button>
        </div>
      </form>
    </dialog>
  );
}

/** The last tool run: its command, exit code and output. */
export function OutputPanel({ name, running, output, error, onClose }: { name: string; running: boolean; output: ToolOutput | null; error: string; onClose: () => void }) {
  const text = output ? [output.stdout, output.stderr].filter(Boolean).join(output.stdout && output.stderr ? '\n' : '') : '';
  const status = running ? '실행 중…' : error ? '실패' : output?.exit_code == null ? '시작함' : output.exit_code === 0 ? '완료 (0)' : `종료 코드 ${output.exit_code}`;
  const bad = !running && (!!error || (output?.exit_code != null && output.exit_code !== 0));
  return (
    <section className="output-panel" aria-label="도구 출력">
      <header>
        <strong>{name}</strong>
        <span className={bad ? 'out-status bad' : 'out-status'}>{status}</span>
        {output && <code className="out-command">{output.command}</code>}
        <button className="ghost small-btn" onClick={onClose} aria-label="출력 닫기">
          닫기
        </button>
      </header>
      {(error || text) && <pre className={error || output?.stderr ? 'out-text has-err' : 'out-text'}>{error || text}</pre>}
    </section>
  );
}

function newTool(): Tool {
  const id = typeof crypto !== 'undefined' && 'randomUUID' in crypto ? crypto.randomUUID() : `t${Date.now()}`;
  return { id, name: '새 도구', program: '', args: '', cwd: '', contexts: ['file'], run: 'capture', prompt: '', confirm: false, refresh: false };
}

/** Add, edit and remove tools: mine (app settings) and the project's (.tome/tools.json, shared). */
export function ToolManager({ set, path, onSavePersonal, onSaveProject, onTrust, onClose }: {
  set: ToolSet;
  path: string;
  onSavePersonal: (tools: Tool[]) => Promise<void>;
  onSaveProject: (tools: Tool[]) => Promise<void>;
  onTrust: () => Promise<void>;
  onClose: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [scope, setScope] = useState<'personal' | 'project'>('personal');
  const [personal, setPersonal] = useState<Tool[]>(set.personal);
  const [project, setProject] = useState<Tool[]>(set.project);
  const [selected, setSelected] = useState<string | null>(set.personal[0]?.id ?? null);
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');
  useEffect(() => {
    dialog.current?.showModal();
  }, []);

  const list = scope === 'personal' ? personal : project;
  const setList = (next: Tool[]) => {
    (scope === 'personal' ? setPersonal : setProject)(next);
    setDirty(true);
  };
  const tool = list.find((t) => t.id === selected) ?? null;
  const update = (patch: Partial<Tool>) => tool && setList(list.map((t) => (t.id === tool.id ? { ...t, ...patch } : t)));

  function switchScope(next: 'personal' | 'project') {
    setScope(next);
    const l = next === 'personal' ? personal : project;
    setSelected(l[0]?.id ?? null);
  }

  async function save() {
    setSaving(true);
    setError('');
    try {
      if (scope === 'personal') await onSavePersonal(personal);
      else await onSaveProject(project);
      setDirty(false);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  async function browse() {
    const picked = await pickFile({ multiple: false, directory: false, title: '실행할 프로그램' });
    if (typeof picked === 'string') update({ program: picked });
  }

  return (
    <dialog ref={dialog} className="tool-manager" onClose={onClose} aria-labelledby="tm-title">
      <header className="tm-head">
        <h2 id="tm-title">커스텀 도구</h2>
        <div className="segmented" role="tablist">
          <label className={scope === 'personal' ? 'active' : ''}>
            <input type="radio" checked={scope === 'personal'} onChange={() => switchScope('personal')} />내 도구
          </label>
          <label className={scope === 'project' ? 'active' : ''}>
            <input type="radio" checked={scope === 'project'} onChange={() => switchScope('project')} />
            프로젝트 도구
          </label>
        </div>
      </header>

      {scope === 'project' && (
        <div className="tm-note">
          <p className="muted">
            작업본의 <code>.tome/tools.json</code>에 저장합니다. 커밋하면 팀이 같은 도구를 씁니다. 저장소의 명령은 누구나 바꿀 수 있으니,
            내용이 바뀔 때마다 확인 후 신뢰해야 실행됩니다.
          </p>
          {set.project_error && <p className="error-line">{set.project_error}</p>}
          {set.project.length > 0 && !set.project_trusted && !dirty && (
            <div className="trust-box">
              <strong>아직 신뢰하지 않은 프로젝트 도구입니다.</strong> 아래 명령을 확인하고 신뢰하세요.
              <ul className="mono">
                {set.project.map((t) => (
                  <li key={t.id}>
                    {t.name}: {t.program} {t.args}
                  </li>
                ))}
              </ul>
              <button className="primary" onClick={() => void onTrust()}>
                확인했고 신뢰합니다
              </button>
            </div>
          )}
        </div>
      )}

      <div className="tm-body">
        <aside className="tm-list">
          <ul>
            {list.map((t) => (
              <li key={t.id}>
                <button className={t.id === selected ? 'tm-item active' : 'tm-item'} onClick={() => setSelected(t.id)}>
                  {t.name || '(이름 없음)'}
                </button>
              </li>
            ))}
          </ul>
          <div className="tm-list-buttons">
            <button
              className="ghost"
              onClick={() => {
                const t = newTool();
                setList([...list, t]);
                setSelected(t.id);
              }}
            >
              추가
            </button>
            <button
              className="ghost"
              disabled={!tool}
              onClick={() => {
                if (!tool) return;
                const copy = { ...tool, id: newTool().id, name: `${tool.name} 복사본` };
                setList([...list, copy]);
                setSelected(copy.id);
              }}
            >
              복제
            </button>
            <button
              className="ghost"
              disabled={!tool}
              onClick={() => {
                if (!tool) return;
                const rest = list.filter((t) => t.id !== tool.id);
                setList(rest);
                setSelected(rest[0]?.id ?? null);
              }}
            >
              삭제
            </button>
          </div>
        </aside>

        {tool ? (
          <div className="tm-form">
            <label className="field">
              <span>이름</span>
              <input value={tool.name} onChange={(e) => update({ name: e.target.value })} />
            </label>
            <label className="field">
              <span>프로그램</span>
              <span className="field-row">
                <input className="mono" value={tool.program} onChange={(e) => update({ program: e.target.value })} placeholder="C:\Program Files\Microsoft VS Code\Code.exe" spellCheck={false} />
                <button type="button" className="ghost" onClick={() => void browse()}>
                  찾아보기
                </button>
              </span>
            </label>
            <label className="field">
              <span>인자</span>
              <input className="mono" value={tool.args} onChange={(e) => update({ args: e.target.value })} placeholder="--goto %f" spellCheck={false} />
            </label>
            <label className="field">
              <span>작업 폴더</span>
              <input className="mono" value={tool.cwd} onChange={(e) => update({ cwd: e.target.value })} placeholder="비우면 작업본 루트($r)" spellCheck={false} />
            </label>
            <fieldset className="field">
              <legend>표시 위치</legend>
              <span className="checks">
                {(['repository', 'file', 'revision'] as ToolContext[]).map((c) => (
                  <label key={c} className="check">
                    <input
                      type="checkbox"
                      checked={tool.contexts.includes(c)}
                      onChange={(e) => update({ contexts: e.target.checked ? [...tool.contexts, c] : tool.contexts.filter((x) => x !== c) })}
                    />
                    {CONTEXT_LABEL[c]}
                  </label>
                ))}
              </span>
            </fieldset>
            <label className="field">
              <span>실행 방식</span>
              <select value={tool.run} onChange={(e) => update({ run: e.target.value as RunMode })}>
                {(Object.keys(RUN_LABEL) as RunMode[]).map((m) => (
                  <option key={m} value={m}>
                    {RUN_LABEL[m]}
                  </option>
                ))}
              </select>
            </label>
            <label className="field">
              <span>실행 전에 물어보기(%a)</span>
              <input value={tool.prompt} onChange={(e) => update({ prompt: e.target.value })} placeholder="비우면 묻지 않음 (예: 커밋 메시지)" />
            </label>
            <span className="checks">
              <label className="check">
                <input type="checkbox" checked={tool.confirm} onChange={(e) => update({ confirm: e.target.checked })} />
                실행 전에 명령 확인
              </label>
              <label className="check">
                <input type="checkbox" checked={tool.refresh} onChange={(e) => update({ refresh: e.target.checked })} />
                끝나면 작업본 새로 고침
              </label>
            </span>
            <details className="vars">
              <summary>쓸 수 있는 변수</summary>
              <dl>
                {VARIABLES.map(([v, d]) => (
                  <div key={v}>
                    <dt className="mono">{v}</dt>
                    <dd>{d}</dd>
                  </div>
                ))}
              </dl>
              <p className="muted">공백이 든 값은 "…"로 묶습니다. 셸을 거치지 않아 파일 이름이 명령을 바꿀 수 없습니다.</p>
            </details>
          </div>
        ) : (
          <p className="muted tm-empty">{scope === 'project' && !path ? '작업본을 먼저 여세요' : '“추가”로 도구를 만드세요'}</p>
        )}
      </div>

      {error && <p className="error-line">{error}</p>}
      <div className="dialog-buttons">
        <button className="ghost" onClick={() => dialog.current?.close()}>
          닫기
        </button>
        <button className="primary" onClick={() => void save()} disabled={!dirty || saving || (scope === 'project' && !path)}>
          {saving ? '저장 중…' : scope === 'project' ? '.tome/tools.json에 저장' : '저장'}
        </button>
      </div>
    </dialog>
  );
}
