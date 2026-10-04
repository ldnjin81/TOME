/** 에셋 클래스에 따라 기본 카드 색상 계열을 정한다. */
export function classTone(cls: string, kind: string) {
  if (kind === 'umap' || cls === 'World') return 'map';
  if (/Blueprint|BlueprintGeneratedClass/.test(cls)) return 'blueprint';
  if (/Sound|MetaSound|Submix/.test(cls)) return 'sound';
  if (/Anim|Pose|Blend|Montage|Chooser|IKR/.test(cls)) return 'anim';
  if (/Material|Texture/.test(cls)) return 'material';
  if (/Enum|Struct|DataTable|DataAsset|Curve|Input/.test(cls)) return 'data';
  return 'other';
}
