//! A React renderer whose host tree is plain objects, serialized to the webview.
//!
//! Host element types are Raycast component names ("List.Item", "Action", …).
//! Function props cannot cross the process boundary; they are sent as
//! `{ $fn: 1 }` and the webview calls them back by node id and prop path.

import { createContext, type ReactNode } from "react";
import Reconciler from "react-reconciler";
import {
  ConcurrentRoot,
  DefaultEventPriority,
  DiscreteEventPriority,
  NoEventPriority,
} from "react-reconciler/constants";

export interface HostNode {
  id: number;
  type: string;
  props: Record<string, any>;
  children: HostNode[];
  hidden?: boolean;
}

export interface Container {
  children: HostNode[];
  onCommit(): void;
}

const nodes = new Map<number, HostNode>();
let nextId = 1;
let updatePriority: number = NoEventPriority;

function propsOf(props: Record<string, any>) {
  const { children: _, ...rest } = props;
  return rest;
}

function create(type: string, props: Record<string, any>): HostNode {
  const node = { id: nextId++, type, props: propsOf(props), children: [] };
  nodes.set(node.id, node);
  return node;
}

function insert(parent: { children: HostNode[] }, child: HostNode, before?: HostNode) {
  const existing = parent.children.indexOf(child);
  if (existing !== -1) parent.children.splice(existing, 1);
  const index = before ? parent.children.indexOf(before) : -1;
  if (index === -1) parent.children.push(child);
  else parent.children.splice(index, 0, child);
}

function remove(parent: { children: HostNode[] }, child: HostNode) {
  const index = parent.children.indexOf(child);
  if (index !== -1) parent.children.splice(index, 1);
}

/// What `ref` resolves to on a host element, e.g. a `Form.TextField` ref.
export const publicInstanceHooks: { create(node: HostNode): unknown } = {
  create: (node) => node,
};

const reconciler = Reconciler({
  rendererPackageName: "heyo",
  rendererVersion: "0.1.0",
  supportsMutation: true,
  supportsPersistence: false,
  supportsHydration: false,
  isPrimaryRenderer: true,

  createInstance: (type: string, props: Record<string, any>) => create(type, props),
  createTextInstance: (text: string) => create("#text", { text }),
  appendInitialChild: (parent: HostNode, child: HostNode) => insert(parent, child),
  appendChild: (parent: HostNode, child: HostNode) => insert(parent, child),
  appendChildToContainer: (container: Container, child: HostNode) => insert(container, child),
  insertBefore: (parent: HostNode, child: HostNode, before: HostNode) => insert(parent, child, before),
  insertInContainerBefore: (container: Container, child: HostNode, before: HostNode) =>
    insert(container, child, before),
  removeChild: (parent: HostNode, child: HostNode) => remove(parent, child),
  removeChildFromContainer: (container: Container, child: HostNode) => remove(container, child),
  detachDeletedInstance: (node: HostNode) => nodes.delete(node.id),
  clearContainer: (container: Container) => {
    container.children.length = 0;
  },
  commitUpdate: (node: HostNode, _type: string, _old: unknown, props: Record<string, any>) => {
    node.props = propsOf(props);
  },
  commitTextUpdate: (node: HostNode, _old: string, text: string) => {
    node.props = { text };
  },
  hideInstance: (node: HostNode) => {
    node.hidden = true;
  },
  unhideInstance: (node: HostNode) => {
    node.hidden = false;
  },
  hideTextInstance: (node: HostNode) => {
    node.hidden = true;
  },
  unhideTextInstance: (node: HostNode) => {
    node.hidden = false;
  },
  resetTextContent: () => {},
  commitMount: () => {},
  finalizeInitialChildren: () => false,
  shouldSetTextContent: () => false,
  getRootHostContext: () => ({}),
  getChildHostContext: (context: object) => context,
  getPublicInstance: (node: HostNode) => publicInstanceHooks.create(node),
  prepareForCommit: () => null,
  resetAfterCommit: (container: Container) => container.onCommit(),
  preparePortalMount: () => {},

  scheduleTimeout: setTimeout,
  cancelTimeout: clearTimeout,
  noTimeout: -1,
  supportsMicrotasks: true,
  scheduleMicrotask: queueMicrotask,

  getCurrentUpdatePriority: () => updatePriority,
  setCurrentUpdatePriority: (priority: number) => {
    updatePriority = priority;
  },
  resolveUpdatePriority: () =>
    updatePriority !== NoEventPriority ? updatePriority : DefaultEventPriority,
  resolveEventType: () => null,
  resolveEventTimeStamp: () => -1.1,
  trackSchedulerEvent: () => {},
  shouldAttemptEagerTransition: () => false,
  requestPostPaintCallback: () => {},

  getInstanceFromNode: () => null,
  beforeActiveInstanceBlur: () => {},
  afterActiveInstanceBlur: () => {},
  prepareScopeUpdate: () => {},
  getInstanceFromScope: () => null,

  maySuspendCommit: () => false,
  maySuspendCommitOnUpdate: () => false,
  maySuspendCommitInSyncRender: () => false,
  preloadInstance: () => true,
  startSuspendingCommit: () => {},
  suspendInstance: () => {},
  waitForCommitToBeReady: () => null,
  getSuspendedCommitReason: () => null,
  NotPendingTransition: null,
  HostTransitionContext: createContext(null),
  resetFormInstance: () => {},
} as any);

/// Renders `element` into a fresh root and returns an unmount function.
export function mount(element: ReactNode, container: Container, onError: (e: unknown) => void) {
  const root = (reconciler as any).createContainer(
    container,
    ConcurrentRoot,
    null,
    false,
    null,
    "",
    onError,
    onError,
    onError,
    () => {},
  );
  reconciler.updateContainer(element, root, null, null);
  return () => reconciler.updateContainer(null, root, null, null);
}

const MAX_DEPTH = 12;

function encode(value: unknown, depth: number): unknown {
  if (typeof value === "function") return { $fn: 1 };
  if (value === null || typeof value !== "object") {
    return typeof value === "bigint" ? Number(value) : value;
  }
  if (depth > MAX_DEPTH) return null;
  if (value instanceof Date) return { $date: value.toISOString() };
  // A React element in a prop the shim did not lift into a slot.
  if ("$$typeof" in value) return null;
  if (Array.isArray(value)) return value.map((v) => encode(v, depth + 1));
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(value)) {
    if (v !== undefined) out[k] = encode(v, depth + 1);
  }
  return out;
}

export const encodeProps = (node: HostNode) => encode(node.props, 0) as Record<string, unknown>;

export const visibleChildren = (node: { children: HostNode[] }) => node.children.filter((c) => !c.hidden);

/// Inverse of the webview's encoding of callback arguments.
export function decode(value: unknown): unknown {
  if (value === null || typeof value !== "object") return value;
  if (Array.isArray(value)) return value.map(decode);
  if ("$date" in value) return new Date((value as { $date: string }).$date);
  return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, decode(v)]));
}

/// Calls the function prop at `path` (e.g. `pagination.onLoadMore`) of a node.
export function dispatch(id: number, path: string, args: unknown[]): unknown {
  let target: any = nodes.get(id)?.props;
  for (const key of path.split(".")) target = target?.[key];
  if (typeof target !== "function") return undefined;
  // Updates from user input render at discrete (synchronous) priority.
  const previous = updatePriority;
  updatePriority = DiscreteEventPriority;
  try {
    return target(...args.map(decode));
  } finally {
    updatePriority = previous;
  }
}

export function nodeById(id: number) {
  return nodes.get(id);
}
