// Markdown detail views and their metadata panels.

import { useContext, useMemo } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import DOMPurify from "dompurify";
import { marked } from "marked";

import { isFilePath } from "./glyph";
import { ExtensionDir, Image } from "./Image";
import { content, cssColor, isFn, sendToRuntime, slot, textOf, type Node } from "./model";

// Markdown comes from extensions, store listings and AI answers: content
// nobody here wrote. It gets text formatting, links and images, and nothing
// that could restyle the palette, submit a form or play media.
const SANITIZE = {
  FORBID_TAGS: ["style", "form", "input", "button", "textarea", "select", "option", "map", "area", "audio", "video", "source", "svg", "math", "iframe", "object", "embed"],
  FORBID_ATTR: ["style", "srcset", "background", "action", "formaction", "ping"],
};

/// Links worth handing to the desktop: web pages and mail. Anything else
/// (files, custom schemes that start other programs) is not opened.
export function isSafeLink(href: unknown): href is string {
  return typeof href === "string" && /^(https?:\/\/|mailto:)/i.test(href.trim());
}

const escapeHtml = (text: string) => text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

// For model output. Raw HTML is shown as the text it is: tags and comments
// would otherwise be dropped or hidden here while still being part of what
// gets copied or pasted. Images become links: an image is fetched as soon as
// it renders, from wherever the model was talked into pointing, which is a
// way to carry the user's text off the machine.
const LITERAL = {
  html: ({ text }: { text: string }) => escapeHtml(text),
  image: ({ href, text }: { href: string; text: string }) => `<a href="${escapeHtml(href)}">[image: ${escapeHtml(text || href)}]</a>`,
};

/// `untrusted` is for text a model wrote (see `LITERAL`).
export function Markdown({ markdown, untrusted }: { markdown: string; untrusted?: boolean }) {
  const extensionDir = useContext(ExtensionDir);
  const html = useMemo(() => {
    const raw = marked.parse(markdown ?? "", { async: false, gfm: true, breaks: false, ...(untrusted ? { renderer: Object.assign(new marked.Renderer(), LITERAL) } : {}) });
    const doc = new DOMParser().parseFromString(raw, "text/html");
    // Relative image paths refer to the extension's assets.
    doc.querySelectorAll("img").forEach((img) => {
      // Raycast sizes images through parameters on their address
      // ("…?raycast-width=320"), on data: addresses as well, where they would
      // otherwise corrupt the image.
      const src = (img.getAttribute("src") ?? "").replace(/[?&]raycast-(width|height|tint-color)=([^&#]*)/g, (_, name, value) => {
        if (name !== "tint-color" && /^\d+$/.test(value)) img.setAttribute(name, value);
        return "";
      });
      img.setAttribute("src", src);
      if (src.startsWith("file://")) img.src = convertFileSrc(decodeURI(src.slice(7)));
      else if (isFilePath(src)) img.src = convertFileSrc(src);
      else if (extensionDir && !/^(https?|data):/.test(src)) img.src = convertFileSrc(`${extensionDir}/assets/${src}`);
    });
    // Sanitized last, so nothing is parsed or rewritten after the check.
    return DOMPurify.sanitize(doc.body.innerHTML, SANITIZE);
  }, [markdown, extensionDir, untrusted]);

  const onClick = (e: React.MouseEvent) => {
    const link = (e.target as HTMLElement).closest("a");
    if (!link) return;
    e.preventDefault();
    const href = link.getAttribute("href");
    if (isSafeLink(href)) sendToRuntime({ type: "open", target: href.trim() });
  };

  return <div className="markdown" onClick={onClick} dangerouslySetInnerHTML={{ __html: html }} />;
}

function ColoredText({ value }: { value: unknown }) {
  const color = value && typeof value === "object" && "color" in value ? cssColor((value as any).color) : undefined;
  return <span style={{ color }}>{textOf(value)}</span>;
}

export function Metadata(props: { node: Node | undefined; dispatch(node: Node, prop: string): void }) {
  if (!props.node) return null;
  return (
    <div className="metadata">
      {content(props.node).map((item) => {
        const p = item.props;
        switch (item.type) {
          case "Metadata.Label":
            return (
              <div key={item.id} className="meta-row">
                <div className="meta-title">{p.title}</div>
                <div className="meta-value">
                  {p.icon != null && <Image image={p.icon} size={16} />}
                  <ColoredText value={p.text} />
                </div>
              </div>
            );
          case "Metadata.Link":
            return (
              <div key={item.id} className="meta-row">
                <div className="meta-title">{p.title}</div>
                <a className="meta-value link" onClick={() => isSafeLink(p.target) && sendToRuntime({ type: "open", target: p.target.trim() })}>
                  {p.text}
                </a>
              </div>
            );
          case "Metadata.TagList":
            return (
              <div key={item.id} className="meta-row">
                <div className="meta-title">{p.title}</div>
                <div className="meta-value tags">
                  {content(item).map((tag) => (
                    <span
                      key={tag.id}
                      className={`tag${isFn(tag.props.onAction) ? " clickable" : ""}`}
                      style={{ color: cssColor(tag.props.color) }}
                      onClick={() => isFn(tag.props.onAction) && props.dispatch(tag, "onAction")}
                    >
                      {tag.props.icon != null && <Image image={tag.props.icon} size={12} />}
                      {tag.props.text}
                    </span>
                  ))}
                </div>
              </div>
            );
          case "Metadata.Separator":
            return <hr key={item.id} className="meta-separator" />;
          default:
            return null;
        }
      })}
    </div>
  );
}

/// `Detail`, and the detail pane of a `List` (`List.Item.Detail`).
export function DetailView(props: { node: Node; dispatch(node: Node, prop: string): void }) {
  const { node } = props;
  const metadata = slot(node, "metadata");
  return (
    <div className={`detail${metadata ? " with-metadata" : ""}`}>
      {node.props.markdown != null && (
        <div className="detail-body">
          <Markdown markdown={node.props.markdown} untrusted={!!node.props.untrusted} />
        </div>
      )}
      {metadata && <Metadata node={metadata} dispatch={props.dispatch} />}
    </div>
  );
}
