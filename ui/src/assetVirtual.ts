export interface VirtualGrid {
  columns: number;
  rowHeight: number;
  first: number;
  end: number;
  previewFirst: number;
  previewEnd: number;
  top: number;
  bottom: number;
}

/** 화면에 걸친 행과 앞뒤 여유 범위를 계산한다. */
export function virtualGrid(count: number, width: number, thumb: number, scrollTop: number, viewportHeight: number, gridTop: number): VirtualGrid {
  const gap = 10;
  const columns = Math.max(1, Math.floor((Math.max(width, thumb) + gap) / (thumb + gap)));
  const columnWidth = (Math.max(width, thumb) - gap * (columns - 1)) / columns;
  const rowHeight = Math.ceil(columnWidth + 42);
  const stride = rowHeight + gap;
  const rows = Math.ceil(count / columns);
  const firstVisible = Math.min(Math.max(0, rows - 1), Math.max(0, Math.floor((scrollTop - gridTop) / stride)));
  const lastVisible = Math.min(rows, Math.ceil((scrollTop + viewportHeight - gridTop) / stride));
  const firstRow = Math.max(0, firstVisible - 2);
  const endRow = Math.min(rows, Math.max(firstRow, lastVisible + 2));
  const extraRows = Math.ceil(viewportHeight / stride);
  return {
    columns,
    rowHeight,
    first: firstRow * columns,
    end: Math.min(count, endRow * columns),
    previewFirst: Math.max(0, firstVisible - extraRows) * columns,
    previewEnd: Math.min(count, (lastVisible + extraRows) * columns),
    top: firstRow * stride,
    bottom: Math.max(0, (rows - endRow) * stride),
  };
}
