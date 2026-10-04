import { describe, expect, it } from 'vitest';
import { isDraft } from './Smartlog';
import { reorderPicks } from './stackLogic';
import type { Revision, StackInfo, Status } from './types';

const revision = (id: string, number: number, branch_id = 'mine'): Revision => ({
  id, number, branch_id, parents: [], message: `메시지 ${id}`, author: '', timestamp: 0, metadata: [],
});
const status: Status = {
  branch_id: 'mine', branch_name: 'mine', revision: 'a', revision_number: 5,
  remote_number: 3, local_ahead: true, remote_ahead: false, merging: '', files: [],
};
const stack = (remote_head: string, drafts: Revision[]): StackInfo => ({ drafts, fork: null, incoming: [], remote_head });
const drafts = [revision('c', 5), revision('b', 4), revision('a', 3)];
const ids = (picks: { id: string }[] | null) => picks?.map((pick) => pick.id);

describe('isDraft', () => {
  it('서버 스택이 있으면 목록에 있는 리비전만 드래프트로 본다', () => {
    expect(isDraft(revision('c', 1), status, stack('remote', [revision('c', 1)]))).toBe(true);
    expect(isDraft(revision('x', 9), status, stack('remote', [revision('c', 1)]))).toBe(false);
  });

  it('서버 스택이 없으면 현재 브랜치와 서버 리비전 번호를 비교한다', () => {
    expect(isDraft(revision('a', 4), status, null)).toBe(true);
    expect(isDraft(revision('b', 3), status, null)).toBe(false);
    expect(isDraft(revision('c', 9, 'other'), status, null)).toBe(false);
  });

  it('로컬 선행 상태가 아니거나 상태가 없으면 드래프트가 아니다', () => {
    expect(isDraft(revision('a', 4), { ...status, local_ahead: false })).toBe(false);
    expect(isDraft(revision('a', 4), undefined)).toBe(false);
  });

  it('서버 head가 비었으면 번호 비교로 돌아간다', () => {
    expect(isDraft(revision('a', 4), status, stack('', []))).toBe(true);
  });
});

describe('reorderPicks', () => {
  it('최신 항목을 다른 항목의 뒤에 놓으면 오래된 순서로 반환한다', () => {
    expect(ids(reorderPicks(drafts, 'c', 'b', 'after'))).toEqual(['a', 'c', 'b']);
  });

  it('가장 오래된 항목을 맨 위로 옮긴다', () => {
    expect(ids(reorderPicks(drafts, 'a', 'c', 'before'))).toEqual(['b', 'c', 'a']);
  });

  it('현재와 같은 위치에 놓으면 restack 요청값을 만들지 않는다', () => {
    expect(reorderPicks(drafts, 'c', 'b', 'before')).toBeNull();
    expect(reorderPicks(drafts, 'b', 'c', 'after')).toBeNull();
    expect(reorderPicks(drafts, 'a', 'a', 'before')).toBeNull();
  });

  it('순서를 바꾸더라도 원본 목록은 유지한다', () => {
    const picks = reorderPicks(drafts, 'a', 'c', 'before');
    expect(picks?.[0].message).toBe('메시지 b');
    expect(ids(drafts)).toEqual(['c', 'b', 'a']);
  });
});
