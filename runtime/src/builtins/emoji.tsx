//! Emoji & Symbols: search in English or Chinese, paste into the focused
//! window. Recently used ones come first.

import path from "node:path";
import { useMemo, useState } from "react";

import en from "emojibase-data/en/compact.json";
import zh from "emojibase-data/zh/compact.json";

import { Action, ActionPanel, Grid } from "../api/components";
import { Icon } from "../api/icon";
import { Clipboard, showHUD } from "../api/system";
import { extensionsDir, readJson, writeJson } from "../context";

interface Emoji {
  char: string;
  name: string;
  keywords: string[];
  group: number;
}

const GROUPS = ["Smileys & Emotion", "People & Body", "", "Animals & Nature", "Food & Drink", "Travel & Places", "Activities", "Objects", "Symbols", "Flags"];

// Characters people look for that are not emoji.
const SYMBOLS: [string, string][] = [
  ["→", "right arrow 右箭头"], ["←", "left arrow 左箭头"], ["↑", "up arrow 上箭头"], ["↓", "down arrow 下箭头"], ["↔", "left right arrow"],
  ["⇒", "double arrow implies"], ["↵", "return enter 回车"], ["⌘", "command cmd"], ["⌥", "option alt"], ["⇧", "shift"], ["⌃", "control ctrl"],
  ["✓", "check mark tick 对勾"], ["✗", "cross mark 叉"], ["•", "bullet 圆点"], ["·", "middle dot 间隔号"], ["…", "ellipsis 省略号"], ["—", "em dash 破折号"], ["–", "en dash"],
  ["©", "copyright 版权"], ["®", "registered"], ["™", "trademark 商标"], ["°", "degree 度"], ["±", "plus minus 正负"], ["×", "multiply times 乘"], ["÷", "divide 除"],
  ["≈", "approximately 约等于"], ["≠", "not equal 不等于"], ["≤", "less or equal 小于等于"], ["≥", "greater or equal 大于等于"], ["∞", "infinity 无穷"], ["√", "square root 根号"],
  ["π", "pi 圆周率"], ["Σ", "sigma sum 求和"], ["Δ", "delta"], ["µ", "micro mu"], ["Ω", "omega ohm"], ["€", "euro 欧元"], ["£", "pound 英镑"], ["¥", "yen yuan 人民币 日元"],
  ["§", "section"], ["¶", "paragraph pilcrow"], ["†", "dagger"], ["‰", "per mille 千分"], ["½", "half 二分之一"], ["¼", "quarter"], ["²", "squared 平方"], ["³", "cubed 立方"],
  ["「", "corner bracket left 直角引号"], ["」", "corner bracket right 直角引号"], ["《", "book title mark left 书名号"], ["》", "book title mark right 书名号"], ["★", "star filled 实心星"], ["☆", "star outline 空心星"],
];

const chinese = new Map(zh.map((e) => [e.hexcode, e]));
const EMOJI: Emoji[] = en
  // Group 2 is skin tone and hair components, which are not used alone.
  .filter((e) => e.group !== undefined && e.group !== 2)
  .map((e) => {
    const local = chinese.get(e.hexcode);
    return { char: e.unicode, name: e.label, group: e.group!, keywords: [local?.label ?? "", ...(e.tags ?? []), ...(local?.tags ?? [])] };
  });

const recentFile = () => path.join(path.dirname(extensionsDir), "emoji-recent.json");

function matches(query: string, name: string, keywords: string[]) {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  const haystack = [name, ...keywords].map((k) => k.toLowerCase());
  return words.every((w) => haystack.some((h) => h.includes(w)));
}

export function EmojiPicker() {
  const [query, setQuery] = useState("");
  const [recent, setRecent] = useState<string[]>(() => readJson<string[]>(recentFile(), []));

  const use = async (char: string, paste: boolean) => {
    const next = [char, ...recent.filter((c) => c !== char)].slice(0, 24);
    setRecent(next);
    writeJson(recentFile(), next);
    if (paste) await Clipboard.paste(char);
    else {
      await Clipboard.copy(char);
      await showHUD(`Copied ${char}`);
    }
  };

  const item = (char: string, name: string, key: string) => (
    <Grid.Item
      key={key}
      content={char}
      title={query ? name : undefined}
      actions={
        <ActionPanel>
          <Action title="Paste" icon={Icon.Clipboard} onAction={() => use(char, true)} />
          <Action title="Copy" icon={Icon.CopyClipboard} onAction={() => use(char, false)} />
        </ActionPanel>
      }
    />
  );

  const sections = useMemo(() => {
    const emoji = query ? EMOJI.filter((e) => matches(query, e.name, e.keywords)) : EMOJI;
    const symbols = query ? SYMBOLS.filter(([, words]) => matches(query, words, [])) : SYMBOLS;
    // While searching, one list in relevance order: exact names first.
    if (query) {
      const q = query.toLowerCase();
      // keywords[0] is the Chinese name.
      const starts = (e: Emoji) => Number(e.name.toLowerCase().startsWith(q) || e.keywords[0].startsWith(q));
      const ranked = [...emoji].sort((a, b) => starts(b) - starts(a));
      return [
        { title: "Results", items: ranked.slice(0, 160).map((e) => item(e.char, e.name, e.char)) },
        { title: "Characters", items: symbols.map(([char, words]) => item(char, words.split(" ")[0], char)) },
      ];
    }
    const names = new Map<string, string>([...EMOJI.map((e) => [e.char, e.name] as const), ...SYMBOLS.map(([c, w]) => [c, w] as const)]);
    return [
      { title: "Recently Used", items: recent.map((char) => item(char, names.get(char) ?? "", `recent-${char}`)) },
      ...GROUPS.map((title, group) => ({ title, items: EMOJI.filter((e) => e.group === group).map((e) => item(e.char, e.name, e.char)) })),
      { title: "Characters", items: symbols.map(([char, words]) => item(char, words, char)) },
    ];
  }, [query, recent]);

  return (
    <Grid columns={10} inset="small" filtering={false} onSearchTextChange={setQuery} searchBarPlaceholder="Search emoji and symbols… (English or 中文)">
      <Grid.EmptyView icon={Icon.Emoji} title="No emoji found" />
      {sections
        .filter((s) => s.items.length)
        .map((s) => (
          <Grid.Section key={s.title} title={s.title}>
            {s.items}
          </Grid.Section>
        ))}
    </Grid>
  );
}
