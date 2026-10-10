import { Component, createContext, createElement, useContext, useMemo, useRef, useState, type ReactNode } from "react";

export interface Navigation {
  push(element: ReactNode, onPop?: () => void): void;
  pop(): void;
}

const NavigationContext = createContext<Navigation | null>(null);

export function useNavigation(): Navigation {
  const navigation = useContext(NavigationContext);
  if (!navigation) throw new Error("useNavigation must be used inside a command");
  return navigation;
}

interface Entry {
  key: number;
  element: ReactNode;
  onPop?: () => void;
}

/// Renders each pushed view inside a `View` host element. Lower views stay
/// mounted (keeping their state); the session sends only the top one.
export function Navigator(props: { root: ReactNode; onPopRoot(): void; expose(nav: Navigation): void }) {
  const [stack, setStack] = useState<Entry[]>([{ key: 0, element: props.root }]);
  const stackRef = useRef(stack);
  stackRef.current = stack;
  const nextKey = useRef(1);

  const navigation = useMemo<Navigation>(
    () => ({
      push(element, onPop) {
        setStack((s) => [...s, { key: nextKey.current++, element, onPop }]);
      },
      pop() {
        const stack = stackRef.current;
        if (stack.length <= 1) return props.onPopRoot();
        stack.at(-1)!.onPop?.();
        stackRef.current = stack.slice(0, -1);
        setStack(stackRef.current);
      },
    }),
    [],
  );
  props.expose(navigation);

  return createElement(
    NavigationContext.Provider,
    { value: navigation },
    stack.map((entry) =>
      createElement("View", { key: entry.key, viewKey: entry.key }, createElement(ErrorBoundary, null, entry.element)),
    ),
  );
}

class ErrorBoundary extends Component<{ children: ReactNode }, { error: unknown }> {
  state = { error: null as unknown };

  static getDerivedStateFromError(error: unknown) {
    return { error };
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    const text = error instanceof Error ? (error.stack ?? error.message) : String(error);
    return createElement("Detail", { markdown: `## Error\n\n\`\`\`\n${text}\n\`\`\`` });
  }
}
