/** 툴바 View 알약에 쓸 요약: 규칙이 없으면 전체, 프리셋과 같으면 그 이름, 아니면 규칙 수. */
export type ViewSummary = { kind: 'all' } | { kind: 'preset'; name: string } | { kind: 'rules'; count: number };

/** ViewDialog와 같은 방식(앞뒤 공백 제거, 빈 줄 무시)으로 규칙을 센다. */
export function viewRules(lines: string[]): string[] {
  return lines.map((l) => l.trim()).filter(Boolean);
}

export function viewSummary(lines: string[], presets: { name: string; lines: string[] }[] = []): ViewSummary {
  const rules = viewRules(lines);
  if (!rules.length) return { kind: 'all' };
  const preset = presets.find((p) => p.lines.length === rules.length && p.lines.every((l, i) => l === rules[i]));
  return preset ? { kind: 'preset', name: preset.name } : { kind: 'rules', count: rules.length };
}
