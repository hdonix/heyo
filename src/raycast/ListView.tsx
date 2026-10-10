// `List` and `Grid`.

import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";

import { DetailView } from "./Detail";
import { Image } from "./Image";
import { ItemIcon } from "../ItemIcon";
import type { ViewMemory } from "./memory";
import { reveal, step as stepped, underPointer, useReveal } from "../selection";
import { SelectionIndicator } from "../SelectionIndicator";
import { content, cssColor, isFn, matchesQuery, relativeTime, slot, textOf, tooltipOf, type Node } from "./model";
import type { BodyProps } from "./CommandView";

interface Group {
  section?: Node;
  items: Node[];
}

function valueOf(v: unknown): unknown {
  return v && typeof v === "object" && !(v instanceof Date) && "value" in v ? (v as { value: unknown }).value : v;
}

function colorOf(v: unknown) {
  return v && typeof v === "object" && "color" in v ? cssColor((v as { color: unknown }).color) : undefined;
}

function display(v: unknown) {
  const value = valueOf(v);
  return value instanceof Date ? relativeTime(value) : textOf(value);
}

function Accessories({ accessories }: { accessories?: any[] }) {
  if (!Array.isArray(accessories)) return null;
  return (
    <span className="accessories">
      {accessories.map((a, i) => (
        <span key={i} className="accessory" title={a.tooltip ?? undefined}>
          {a.icon != null && <Image image={a.icon} size={14} />}
          {a.tag != null && (
            <span className="tag" style={{ color: colorOf(a.tag) }}>
              {display(a.tag)}
            </span>
          )}
          {a.text != null && <span style={{ color: colorOf(a.text) }}>{display(a.text)}</span>}
          {a.date != null && <span style={{ color: colorOf(a.date) }}>{display(a.date)}</span>}
        </span>
      ))}
    </span>
  );
}

const GRID_COLUMNS: Record<string, number> = { small: 8, medium: 5, large: 3 };
const INSETS: Record<string, string> = { zero: "0", small: "6%", medium: "12%", large: "20%" };

function GridContent({ item, grid }: { item: Node; grid: Node }) {
  const raw = item.props.content;
  const value = valueOf(raw);
  const fit = grid.props.fit ?? "contain";
  const style = {
    aspectRatio: String(grid.props.aspectRatio ?? "1").replace("/", " / "),
    padding: INSETS[grid.props.inset ?? "zero"],
  };
  if (value && typeof value === "object" && "color" in value && !("source" in value)) {
    return <div className="grid-tile" style={{ ...style, background: cssColor((value as { color: unknown }).color) }} />;
  }
  return (
    <div className={`grid-tile fit-${fit}`} style={style} title={tooltipOf(raw)}>
      <Image image={value} size={48} className="grid-image" />
    </div>
  );
}

interface RowProps {
  item: Node;
  grid: Node;
  selected: boolean;
  showDetail: boolean;
  onSelect(item: Node): void;
  onRun(item: Node): void;
}

// Rows re-render only when their node changed (the tree store keeps
// unchanged nodes identical) or their selection did.
const ListRow = memo(function ListRow({ item, selected, showDetail, onSelect, onRun }: RowProps) {
  return (
    <div className={`item${selected ? " selected" : ""}`} {...underPointer(() => onSelect(item))} onClick={() => onRun(item)}>
      {item.props.icon != null ? <ItemIcon icon={item.props.icon} title={textOf(item.props.title)} variant="plain" /> : null}
      <span className="title" title={tooltipOf(item.props.title)}>
        {textOf(item.props.title)}
      </span>
      {!showDetail && item.props.subtitle != null && (
        <span className="subtitle" title={tooltipOf(item.props.subtitle)}>
          {textOf(item.props.subtitle)}
        </span>
      )}
      {!showDetail && <Accessories accessories={item.props.accessories} />}
    </div>
  );
});

const GridCell = memo(function GridCell({ item, grid, selected, onSelect, onRun }: RowProps) {
  return (
    <div className={`grid-item${selected ? " selected" : ""}`} {...underPointer(() => onSelect(item))} onClick={() => onRun(item)}>
      <GridContent item={item} grid={grid} />
      {item.props.title && <div className="grid-title">{textOf(item.props.title)}</div>}
      {item.props.subtitle && <div className="grid-subtitle">{textOf(item.props.subtitle)}</div>}
    </div>
  );
});

interface GroupProps {
  group: { section?: Node; items: Node[] };
  /// The grid's layout props, for a grid.
  grid?: Node;
  columns: number;
  selectedId: number | null;
  showDetail: boolean;
  onSelect(item: Node): void;
  onRun(item: Node): void;
}

const GroupRows = memo(function GroupRows({ group, grid, columns, selectedId, showDetail, onSelect, onRun }: GroupProps) {
  const Row = grid ? GridCell : ListRow;
  return (
    <div className="group">
      {group.section?.props.title && (
        <div className="section-title">
          {group.section.props.title}
          {group.section.props.subtitle && <span className="section-subtitle">{group.section.props.subtitle}</span>}
        </div>
      )}
      <div
        className={grid ? "grid" : undefined}
        style={grid ? { gridTemplateColumns: `repeat(${group.section?.props.columns ?? columns}, 1fr)` } : undefined}
      >
        {group.items.map((item) => (
          <Row key={item.id} item={item} grid={grid!} selected={item.id === selectedId} showDetail={showDetail} onSelect={onSelect} onRun={onRun} />
        ))}
      </div>
    </div>
  );
});

export function ListView({ node, query, dispatch, keys, onActions, memory }: BodyProps & { memory: ViewMemory }) {
  const grid = node.type === "Grid";
  const prefix = grid ? "Grid" : "List";
  const filtering =
    node.props.filtering !== undefined ? !!node.props.filtering : !isFn(node.props.onSearchTextChange);

  const groups = useMemo(() => {
    const out: Group[] = [];
    let loose: Group | undefined;
    for (const child of content(node)) {
      if (child.type === `${prefix}.Section`) {
        loose = undefined;
        out.push({ section: child, items: content(child).filter((c) => c.type === `${prefix}.Item`) });
      } else if (child.type === `${prefix}.Item`) {
        if (!loose) out.push((loose = { items: [] }));
        loose.items.push(child);
      }
    }
    const filtered =
      filtering && query
        ? out.map((g) => ({
            ...g,
            items: g.items.filter((i) => matchesQuery(query, textOf(i.props.title), i.props.keywords)),
          }))
        : out;
    return filtered.filter((g) => g.items.length);
  }, [node, query, filtering, prefix]);

  const items = useMemo(() => groups.flatMap((g) => g.items), [groups]);
  const emptyView = content(node).find((c) => c.type === `${prefix}.EmptyView`);
  const columns = node.props.columns ?? GRID_COLUMNS[node.props.itemSize ?? "medium"] ?? 5;

  // A list come back to (a view was pushed over it) has the selection it
  // had: the runtime kept its nodes, and sends them again under their ids.
  const [returning] = useState(() => memory.selectedId != null);
  const [selectedId, setSelectedId] = useState<number | null>(memory.selectedId);
  const found = items.findIndex((i) => i.id === selectedId);
  const index = found === -1 ? 0 : found;
  const selected = items[index];
  memory.selectedId = selected?.id ?? null;

  // A new query starts from the top. (Not the query the list starts with.)
  const lastQuery = useRef(query);
  useEffect(() => {
    if (lastQuery.current === query) return;
    lastQuery.current = query;
    setSelectedId(null);
  }, [query]);

  // `selectedItemId` selects programmatically, possibly before items arrive.
  // Coming back, what it asked for before the view was left has been
  // answered already, and the user may have moved on from it.
  const wanted = node.props.selectedItemId;
  const answered = useRef({ wanted: returning ? memory.wanted : undefined, count: items.length });
  useEffect(() => {
    memory.wanted = wanted;
    const same = answered.current.wanted === wanted && answered.current.count === items.length;
    answered.current = { wanted, count: items.length };
    if (same || wanted == null) return;
    const item = items.find((i) => i.props.id === wanted);
    if (item) setSelectedId(item.id);
  }, [wanted, items.length]);

  const reported = useRef<unknown>(undefined);
  const selectionId = selected?.props.id ?? null;
  useEffect(() => {
    if (!isFn(node.props.onSelectionChange) || reported.current === selectionId) return;
    reported.current = selectionId;
    dispatch(node, "onSelectionChange", selectionId);
  }, [selectionId]);

  // Infinite scrolling: ask for more when the selection nears the end.
  const requestedAt = useRef(-1);
  const hasMore = !!node.props.pagination?.hasMore;
  useEffect(() => {
    if (!hasMore || node.props.isLoading || index < items.length - 5 || requestedAt.current === items.length) return;
    requestedAt.current = items.length;
    dispatch(node, "pagination.onLoadMore");
  }, [index, items.length, hasMore, node.props.isLoading]);

  const listRef = useReveal<HTMLDivElement>([index, selected?.id]);
  // Back where it was scrolled to; and should the list have changed
  // meanwhile, the selection is in view all the same.
  useLayoutEffect(() => {
    const list = listRef.current;
    if (!list || !memory.scrollTop) return;
    list.scrollTop = memory.scrollTop;
    const row = list.querySelector<HTMLElement>(".selected");
    if (row) reveal(list, row);
  }, []);

  const actions =
    slot(selected, "actions") ?? (items.length ? undefined : slot(emptyView, "actions")) ?? slot(node, "actions");
  useEffect(() => onActions(actions), [actions]);

  // Key presses can arrive faster than renders; step from the latest index.
  const indexRef = useRef(index);
  indexRef.current = index;
  keys.quickLook = selected?.props.quickLook;
  keys.current = (e) => {
    const step = (delta: number) => {
      if (!items.length) return true;
      const next = stepped(indexRef.current, delta, items.length, false);
      indexRef.current = next;
      setSelectedId(items[next].id);
      return true;
    };
    if (e.key === "ArrowDown" || (e.ctrlKey && e.code === "KeyN")) return step(grid ? columns : 1);
    if (e.key === "ArrowUp" || (e.ctrlKey && e.code === "KeyP")) return step(grid ? -columns : -1);
    if (grid && e.key === "ArrowRight" && !e.ctrlKey) return step(1);
    if (grid && e.key === "ArrowLeft" && !e.ctrlKey) return step(-1);
    if (e.key === "PageDown") return step(grid ? columns * 3 : 8);
    if (e.key === "PageUp") return step(grid ? -columns * 3 : -8);
    return false;
  };

  const showDetail = !grid && node.props.isShowingDetail;
  const onSelect = useCallback((item: Node) => setSelectedId(item.id), []);
  const onRun = useCallback((item: Node) => {
    setSelectedId(item.id);
    keys.runPrimary?.(slot(item, "actions"));
  }, []);
  // Cells read only layout props from the grid; keep them stable across renders.
  const { aspectRatio, fit, inset } = node.props;
  const gridProps = useMemo(() => ({ ...node, props: { aspectRatio, fit, inset } }), [aspectRatio, fit, inset]);
  const detail = showDetail ? slot(selected, "detail") : undefined;

  if (!items.length) {
    if (emptyView) {
      return (
        <div className="empty-view">
          {emptyView.props.icon != null && <Image image={emptyView.props.icon} size={48} />}
          <div className="empty-title">{emptyView.props.title ?? "No Results"}</div>
          {emptyView.props.description && <div className="empty-description">{emptyView.props.description}</div>}
        </div>
      );
    }
    // Still loading: rows standing in for what is coming.
    if (node.props.isLoading) {
      return (
        <div className="skeleton-list" aria-busy>
          {[46, 62, 38, 54, 44, 58].map((width, i) => (
            <div key={i} className="skeleton-row" style={{ "--skeleton-width": `${width}%` } as React.CSSProperties} />
          ))}
        </div>
      );
    }
    return (
      <div className="empty-view">
        <div className="empty-title">No Results</div>
        <div className="empty-description">Try a different search.</div>
      </div>
    );
  }

  return (
    <div className={`body${showDetail ? " split" : ""}`}>
      <div className="list" ref={listRef} onScroll={(e) => (memory.scrollTop = e.currentTarget.scrollTop)}>
        {!grid && <SelectionIndicator resetKey={items.length} />}
        {groups.map((group, gi) => (
          <GroupRows
            key={group.section?.id ?? `g${gi}`}
            group={group}
            grid={grid ? gridProps : undefined}
            columns={columns}
            // Only the group the selection is in hears of it: the others,
            // and their hundreds of rows, have nothing to render again.
            selectedId={selected && group.items.includes(selected) ? selected.id : null}
            showDetail={!!showDetail}
            onSelect={onSelect}
            onRun={onRun}
          />
        ))}
      </div>
      {showDetail && (
        <div className="detail-pane">
          {detail && <DetailView node={detail} dispatch={(n, p) => dispatch(n, p)} />}
        </div>
      )}
    </div>
  );
}
