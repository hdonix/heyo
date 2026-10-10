//! The icon at the start of a row, of one size and weight whatever it is.
//!
//! In the root search, as in Raycast: an application's own icon as it
//! comes, a command's symbol in white on a tile of its own colour, and where
//! there is no icon the initial of the title. Inside a command's own list,
//! symbols are plain: there they are labels, not identities.

import { Image, isSymbol } from "./raycast/Image";

const TILE_COLORS = ["blue", "green", "magenta", "orange", "purple", "red", "yellow"];

/// The same colour for the same name, every time.
export function tileColor(name: string) {
  let hash = 0;
  for (const char of name) hash = (hash * 31 + char.codePointAt(0)!) >>> 0;
  return `var(--raycast-${TILE_COLORS[hash % TILE_COLORS.length]})`;
}

export function ItemIcon(props: { icon: unknown; title: string; variant?: "tile" | "plain"; colorKey?: string }) {
  const color = { "--tile-color": tileColor(props.colorKey ?? props.title) } as React.CSSProperties;
  if (props.icon == null || props.icon === "") {
    const initial = [...props.title.trim()][0] ?? "?";
    return (
      <span className="icon-tile initial" style={color} aria-hidden>
        {initial}
      </span>
    );
  }
  if (!isSymbol(props.icon)) return <Image image={props.icon} className="icon" size={24} />;
  if (props.variant === "plain") {
    return (
      <span className="icon-plain">
        <Image image={props.icon} size={18} />
      </span>
    );
  }
  return (
    <span className="icon-tile symbol" style={color}>
      <Image image={props.icon} size={15} />
    </span>
  );
}
