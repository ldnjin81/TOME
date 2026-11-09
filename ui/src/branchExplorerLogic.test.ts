import { describe, expect, it } from 'vitest';
import { adjacentNode, clampZoom, HISTORY_STEP_DAYS, moreHistory, recentPosition, scrollBehavior, switchAction, visibleLanes, type ExplorerLane, type ExplorerNode } from './branchExplorerLogic';
const lanes = ['main','feature','child','other'].map((id,i) => ({ branch: { id, name: id }, parent: [null,'main','feature','main'][i], depth: [0,1,2,1][i], parent_source: 'stack' })) as ExplorerLane[];
const nodes = [{ revision:{id:'a'},lane:0,x:10 },{revision:{id:'b'},lane:0,x:50},{revision:{id:'c'},lane:2,x:30},{revision:{id:'d'},lane:3,x:70}] as ExplorerNode[];
describe('브랜치 탐색기', () => {
  it('접힌 트리는 자손 레인까지 숨긴다', () => {
    expect(visibleLanes(lanes,new Set(['feature']),'')).toEqual([0,1,3]);
    expect(visibleLanes(lanes,new Set(['main']),'')).toEqual([0]);
  });
  it('검색 시 조상을 유지하고 접힘을 무시한다', () => {
    expect(visibleLanes(lanes,new Set(['main']),'CHILD')).toEqual([0,1,2]);
    expect(visibleLanes(lanes,new Set(),'missing')).toEqual([]);
  });
  it('같은 레인의 시간순 및 비어 있지 않은 인접 레인으로 이동한다', () => {
    expect(adjacentNode(nodes,[0,1,2,3],'a','ArrowRight')?.revision.id).toBe('b');
    expect(adjacentNode(nodes,[0,1,2,3],'b','ArrowLeft')?.revision.id).toBe('a');
    expect(adjacentNode(nodes,[0,1,2,3],'b','ArrowDown')?.revision.id).toBe('c');
    expect(adjacentNode(nodes,[0,3],'b','ArrowDown')?.revision.id).toBe('d');
    expect(adjacentNode(nodes,[0,1,2,3],'a','ArrowUp')).toBeUndefined();
  });
  it('확대 범위를 제한한다', () => { expect(clampZoom(.01)).toBe(.25); expect(clampZoom(10)).toBe(4); });
});

it('최신 리비전이 깊은 레인에 있어도 초기 화면 안에 둔다', () => {
  const recent = [{x:100,lane:0,revision:{timestamp:1}}, {x:8500,lane:55,revision:{timestamp:2}}] as ExplorerNode[];
  const position = recentPosition(recent, 700, 500);
  expect(position.left).toBe(8150);
  expect(position.top).toBe(3009);
  expect(clampZoom(700/8570)).toBe(.25);
  expect(recentPosition([], 700, 500)).toEqual({left:0,top:0});
});

describe('브랜치 탐색기 0.13 동작', () => {
  it('기록 범위를 90일씩 넓히고 전체는 그대로 둔다', () => {
    expect(HISTORY_STEP_DAYS).toBe(90);
    expect(moreHistory(90)).toBe(180);
    expect(moreHistory(null)).toBeNull();
  });
  it('동작 줄이기면 즉시 스크롤한다', () => {
    expect(scrollBehavior(true)).toBe('auto');
    expect(scrollBehavior(false)).toBe('smooth');
  });
  it('더블클릭 전환: 현재·작업 중은 무시, 보관·병합 중은 막는다', () => {
    const b = (name: string, archived = false) => ({ name, archived });
    expect(switchAction(b('feature'), 'main', false, false)).toBe('switch');
    expect(switchAction(b('main'), 'main', false, false)).toBe('none');
    expect(switchAction(b('feature'), 'main', false, true)).toBe('none');
    expect(switchAction(b('old', true), 'main', false, false)).toBe('archived');
    expect(switchAction(b('feature'), 'main', true, false)).toBe('merging');
  });
});
