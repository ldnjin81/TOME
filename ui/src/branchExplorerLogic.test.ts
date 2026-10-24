import { describe, expect, it } from 'vitest';
import { adjacentNode, clampZoom, recentPosition, visibleLanes, type ExplorerLane, type ExplorerNode } from './branchExplorerLogic';
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
