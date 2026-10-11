// Renders a Raycast `Image.ImageLike`: an icon name, URL, file path, asset
// name, emoji, or `{ source, tintColor, mask, fallback }`.

import { createContext, useContext, useState } from "react";
import { icons, type LucideIcon } from "lucide-react";
import { assetUrl } from "../host";
import { isDark } from "../theme";

import { isFilePath, isGlyph } from "./glyph";
import { iconAliases } from "./iconAliases";
import { cssColor } from "./model";

/// Directory of the running extension, for resolving asset names.
export const ExtensionDir = createContext("");

const pascal = (s: string) =>
  s
    .split("-")
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join("");

function lucideIcon(name: string): { Icon: LucideIcon; filled: boolean } | undefined {
  const base = name.replace(/-16$/, "");
  const unfilled = base.replace(/-filled$/, "");
  const target = iconAliases[base] ?? iconAliases[unfilled] ?? pascal(unfilled);
  const Icon = icons[target as keyof typeof icons] as LucideIcon | undefined;
  return Icon && { Icon, filled: base !== unfilled };
}

function sourceOf(source: unknown): string | undefined {
  if (typeof source === "string") return source;
  if (source && typeof source === "object") {
    // The one drawn for the scheme in use.
    const themed = source as { light?: string; dark?: string };
    return isDark() ? (themed.dark ?? themed.light) : (themed.light ?? themed.dark);
  }
  return undefined;
}

interface Resolved {
  src?: string;
  tint?: string;
  mask?: string;
  fallback?: string;
}

function resolve(image: unknown): Resolved {
  if (typeof image === "string") return { src: image };
  if (!image || typeof image !== "object") return {};
  const obj = image as Record<string, unknown>;
  if ("fileIcon" in obj) return { src: "document-16" };
  if ("source" in obj) {
    return {
      src: sourceOf(obj.source),
      tint: cssColor(obj.tintColor),
      mask: obj.mask as string | undefined,
      fallback: sourceOf(obj.fallback),
    };
  }
  if ("value" in obj) return resolve(obj.value);
  return {};
}

/// Whether an image is a symbol (a named icon, a glyph, or a template that
/// is tinted) rather than a picture with colours of its own.
export function isSymbol(image: unknown): boolean {
  const { src, tint } = resolve(image);
  if (!src) return false;
  return !!tint || /^[a-z0-9-]+-16$/.test(src) || isGlyph(src);
}

export function Image(props: { image: unknown; size?: number; className?: string; title?: string }) {
  const extensionDir = useContext(ExtensionDir);
  const [failed, setFailed] = useState(false);
  const { src: primary, tint, mask, fallback } = resolve(props.image);
  const src = failed ? fallback : primary;
  const size = props.size ?? 20;
  const className = ["image", props.className].filter(Boolean).join(" ");
  if (!src) return <span className={className} style={{ width: size, height: size }} />;

  if (/^[a-z0-9-]+-16$/.test(src)) {
    const number = /^number-(\d+)-16$/.exec(src);
    if (number) {
      return (
        <span className={`${className} glyph`} style={{ width: size, height: size, color: tint }} title={props.title}>
          {number[1]}
        </span>
      );
    }
    const icon = lucideIcon(src);
    if (icon) {
      return (
        <icon.Icon
          className={className}
          size={size}
          color={tint ?? "currentColor"}
          fill={icon.filled ? (tint ?? "currentColor") : "none"}
          aria-label={props.title}
        />
      );
    }
  }

  let url: string;
  if (src.startsWith("data:image/svg+xml,") && src.includes("<")) {
    // Raw SVG markup; an unescaped `#` would end the URL.
    url = `data:image/svg+xml,${encodeURIComponent(src.slice(19))}`;
  } else if (/^(https?|data|blob):/.test(src)) url = src;
  else if (isFilePath(src)) url = assetUrl(src);
  else if (isGlyph(src)) {
    // An emoji or a short symbol.
    return (
      <span className={`${className} glyph`} style={{ width: size, height: size, fontSize: size * 0.85 }} title={props.title}>
        {src}
      </span>
    );
  } else url = extensionDir ? assetUrl(`${extensionDir}/assets/${src}`) : src;

  const radius = mask === "circle" ? "50%" : mask === "roundedRectangle" ? "22%" : undefined;
  if (tint) {
    // A tinted image is a template: its alpha channel masks the tint color.
    const maskImage = `url("${url.replace(/"/g, "%22")}") center / contain no-repeat`;
    return (
      <span
        className={className}
        title={props.title}
        style={{ width: size, height: size, background: tint, mask: maskImage, WebkitMask: maskImage, borderRadius: radius }}
      />
    );
  }
  return (
    <img
      className={className}
      src={url}
      alt=""
      title={props.title}
      width={size}
      height={size}
      style={{ borderRadius: radius }}
      onError={() => fallback && !failed && setFailed(true)}
    />
  );
}
