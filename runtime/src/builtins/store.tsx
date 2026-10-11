//! Built-in commands for browsing the Raycast Store and managing extensions.

import fs from "node:fs";
import path from "node:path";
import { useEffect, useState } from "react";

import { Color } from "../api";
import { Action, ActionPanel, Detail, List } from "../api/components";
import { Icon } from "../api/icon";
import { useNavigation } from "../api/navigation";
import { confirmAlert, showToast, Toast } from "../api/system";
import { extensionsDir, installedExtensions, readJson, readManifest, writeJson, type Extension } from "../context";
import { registered, unregister } from "../dev/registry";
import { request } from "../protocol";
import { extract } from "../unzip";
import { PreferencesForm } from "./setup";

const API = "https://www.raycast.com/api/v1";

interface Listing {
  id: string;
  name: string;
  title: string;
  description: string;
  download_count: number;
  download_url: string;
  store_url: string;
  readme_url?: string;
  readme_assets_path?: string;
  platforms?: string[];
  icons: { light?: string | null; dark?: string | null };
  owner: { handle: string };
  author: { name: string; handle: string };
  commands: { name: string; title: string; mode: string }[];
  commit_sha?: string;
}

async function getJson<T>(url: string, signal?: AbortSignal): Promise<T> {
  const res = await fetch(url, { signal });
  if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
  return res.json() as Promise<T>;
}

async function download(listing: Listing) {
  let res = await fetch(listing.download_url);
  if (!res.ok) {
    // The listing's pre-signed URL expires; ask for a fresh one.
    const fresh = await getJson<Listing>(`${API}/extensions/${listing.owner.handle}/${listing.name}`);
    res = await fetch(fresh.download_url);
  }
  if (!res.ok) throw new Error(`Download failed: ${res.status} ${res.statusText}`);
  return Buffer.from(await res.arrayBuffer());
}

async function install(listing: Listing, update = false) {
  const [doing, done] = update ? ["Updating", "Updated"] : ["Installing", "Installed"];
  const toast = await showToast({ style: Toast.Style.Animated, title: `${doing} ${listing.title}…` });
  fs.mkdirSync(extensionsDir, { recursive: true });
  const staging = fs.mkdtempSync(path.join(extensionsDir, ".install-"));
  try {
    extract(await download(listing), staging);
    const manifest = readManifest(staging);
    // The name becomes a directory that is replaced on install: it must be a
    // plain name ("." and ".." would point at Heyo's own data).
    if (!/^[A-Za-z0-9][\w-]*$/.test(manifest.name)) throw new Error(`Invalid extension name: ${manifest.name}`);
    writeJson(path.join(staging, ".heyo-store.json"), {
      owner: listing.owner.handle,
      name: listing.name,
      commit: listing.commit_sha,
      installedAt: new Date().toISOString(),
    });
    const dest = path.join(extensionsDir, manifest.name);
    if (path.dirname(dest) !== path.resolve(extensionsDir)) throw new Error(`Invalid extension name: ${manifest.name}`);
    fs.rmSync(dest, { recursive: true, force: true });
    fs.renameSync(staging, dest);
    await request("host.reload");
    toast.style = Toast.Style.Success;
    toast.title = `${done} ${listing.title}`;
    toast.message = update ? undefined : "Its commands are now in the root search.";
    return true;
  } catch (e) {
    fs.rmSync(staging, { recursive: true, force: true });
    toast.style = Toast.Style.Failure;
    toast.title = `Failed to ${update ? "update" : "install"} ${listing.title}`;
    toast.message = e instanceof Error ? e.message : String(e);
    return false;
  }
}

/// Where an installed extension came from in the store, and which commit.
function storeOrigin(extension: Extension): { owner: string; name: string; commit?: string } | null {
  const saved = readJson<{ owner?: string; name?: string; commit?: string } | null>(
    path.join(extension.dir, ".heyo-store.json"),
    null,
  );
  if (saved?.owner && saved.name) return { owner: saved.owner, name: saved.name, commit: saved.commit };
  // Installed by hand: the manifest names the owner (or author).
  const owner = extension.manifest.owner ?? extension.manifest.author;
  return owner ? { owner, name: extension.manifest.name } : null;
}

/// Installed extension name → installed commit (if known).
const installedCommits = () =>
  new Map(installedExtensions().map((e) => [e.manifest.name, storeOrigin(e)?.commit] as const));

/// Only extensions installed from the store know their commit; for others
/// there is nothing to compare, so no update is claimed.
const hasUpdate = (installedCommit: string | undefined, listing: Listing) =>
  !!installedCommit && !!listing.commit_sha && installedCommit !== listing.commit_sha;

/// Store listings with a different commit than the installed one, by extension directory.
async function checkUpdates(extensions: Extension[]): Promise<Map<string, Listing>> {
  const updates = new Map<string, Listing>();
  const queue = [...extensions];
  const worker = async () => {
    for (let ext = queue.shift(); ext; ext = queue.shift()) {
      const origin = storeOrigin(ext);
      if (!origin) continue;
      try {
        const listing = await getJson<Listing>(`${API}/extensions/${origin.owner}/${origin.name}`);
        if (hasUpdate(origin.commit, listing)) updates.set(ext.dir, listing);
      } catch {
        // Not in the store (or offline): nothing to update.
      }
    }
  };
  await Promise.all(Array.from({ length: 6 }, worker));
  return updates;
}
const formatCount = (n: number) => Intl.NumberFormat("en", { notation: "compact" }).format(n);
const runnable = (listing: Listing) => listing.commands.filter((c) => c.mode !== "menu-bar");

function ListingActions(props: {
  listing: Listing;
  installed: boolean;
  update?: boolean;
  onInstalled(): void;
  details?: boolean;
}) {
  const { listing } = props;
  return (
    <ActionPanel>
      <Action
        title={props.update ? "Update Extension" : props.installed ? "Reinstall Extension" : "Install Extension"}
        icon={Icon.Download}
        onAction={async () => {
          await install(listing, props.update);
          props.onInstalled();
        }}
      />
      {props.details && (
        <Action.Push
          title="Show Details"
          icon={Icon.Sidebar}
          shortcut={{ modifiers: ["cmd"], key: "d" }}
          target={<ListingDetail listing={listing} installed={props.installed} onInstalled={props.onInstalled} />}
        />
      )}
      <Action.OpenInBrowser url={listing.store_url} shortcut={{ modifiers: ["cmd"], key: "o" }} />
    </ActionPanel>
  );
}

function ListingDetail(props: { listing: Listing; installed: boolean; onInstalled(): void }) {
  const { listing } = props;
  const [readme, setReadme] = useState<string>();

  useEffect(() => {
    if (!listing.readme_url) return setReadme("");
    // The listing links GitHub's HTML view; fetch the raw file instead.
    fetch(listing.readme_url.replace(/^https:\/\/github\.com\/([^/]+\/[^/]+)\/(?:tree|blob)\//, "https://raw.githubusercontent.com/$1/"))
      .then((r) => (r.ok ? r.text() : ""))
      .then((text) => {
        // README images use paths relative to the extension's source folder.
        const assets = (listing.readme_assets_path ?? "").replace(/\/+$/, "/");
        setReadme(text.replace(/\]\((?!https?:|#)\.?\/?([^)]+)\)/g, (_m, p) => `](${assets}${p})`));
      })
      .catch(() => setReadme(""));
  }, []);

  return (
    <Detail
      isLoading={readme === undefined}
      navigationTitle={listing.title}
      markdown={`# ${listing.title}\n\n${listing.description}\n\n${readme ?? ""}`}
      metadata={
        <Detail.Metadata>
          <Detail.Metadata.Label title="Author" text={listing.author.name} />
          <Detail.Metadata.Label title="Downloads" text={formatCount(listing.download_count)} />
          <Detail.Metadata.TagList title="Platforms">
            {(listing.platforms ?? []).map((p) => (
              <Detail.Metadata.TagList.Item key={p} text={p} />
            ))}
          </Detail.Metadata.TagList>
          <Detail.Metadata.Separator />
          {listing.commands.map((c) => (
            <Detail.Metadata.Label key={c.name} title={c.title} text={c.mode} />
          ))}
        </Detail.Metadata>
      }
      actions={<ListingActions {...props} />}
    />
  );
}

export function Store() {
  const [query, setQuery] = useState("");
  const [listings, setListings] = useState<Listing[]>([]);
  const [loading, setLoading] = useState(true);
  const [installed, setInstalled] = useState(installedCommits);

  useEffect(() => {
    const controller = new AbortController();
    setLoading(true);
    const timer = setTimeout(
      async () => {
        try {
          const q = query.trim();
          const url = q
            ? `${API}/store_listings/search?q=${encodeURIComponent(q)}&per_page=50`
            : `${API}/store_listings?per_page=50`;
          setListings((await getJson<{ data: Listing[] }>(url, controller.signal)).data ?? []);
          setLoading(false);
        } catch (e) {
          if (controller.signal.aborted) return;
          setLoading(false);
          showToast({ style: Toast.Style.Failure, title: "Could not reach the Raycast Store", message: e instanceof Error ? e.message : String(e) });
        }
      },
      query ? 250 : 0,
    );
    return () => {
      clearTimeout(timer);
      controller.abort();
    };
  }, [query]);

  const refresh = () => setInstalled(installedCommits());

  return (
    <List isLoading={loading} searchBarPlaceholder="Search the Raycast Store…" onSearchTextChange={setQuery}>
      {listings.map((listing) => {
        const commands = runnable(listing).length;
        const macOnly = listing.platforms?.length === 1 && listing.platforms[0] === "macOS";
        const isInstalled = installed.has(listing.name);
        const update = isInstalled && hasUpdate(installed.get(listing.name), listing);
        return (
          <List.Item
            key={listing.id}
            title={listing.title}
            subtitle={listing.description}
            icon={listing.icons.dark || listing.icons.light || Icon.Box}
            accessories={[
              ...(update
                ? [{ tag: { value: "Update", color: Color.Orange } }]
                : isInstalled
                  ? [{ tag: { value: "Installed", color: Color.Green } }]
                  : []),
              ...(macOnly ? [{ tag: { value: "macOS", color: Color.Orange }, tooltip: "May rely on macOS" }] : []),
              { text: `${commands} command${commands === 1 ? "" : "s"}` },
              { icon: Icon.Download, text: formatCount(listing.download_count) },
            ]}
            actions={
              <ListingActions listing={listing} installed={isInstalled} update={update} onInstalled={refresh} details />
            }
          />
        );
      })}
    </List>
  );
}

/// The extensions `heyo dev` is building, with where their sources are.
/// They are not from the store and are never looked up or updated there.
function developmentExtensions(): (Extension & { source: string })[] {
  return registered().flatMap((entry) => {
    try {
      return [{ dir: entry.dir, manifest: readManifest(entry.dir), source: entry.source }];
    } catch {
      // Registered but never built.
      return [];
    }
  });
}

function ConfigureExtension({ extension }: { extension: Extension }) {
  const { pop } = useNavigation();
  return <PreferencesForm extension={extension} onSaved={pop} />;
}

export function Extensions() {
  const [extensions, setExtensions] = useState(installedExtensions);
  const [developed, setDeveloped] = useState(developmentExtensions);
  const [updates, setUpdates] = useState<Map<string, Listing>>();

  const check = () => checkUpdates(installedExtensions()).then(setUpdates);
  useEffect(() => {
    check();
  }, []);

  const reload = () => {
    setExtensions(installedExtensions());
    setDeveloped(developmentExtensions());
    check();
  };

  const update = async (listings: Listing[]) => {
    for (const listing of listings) await install(listing, true);
    reload();
  };

  const uninstall = async (extension: Extension) => {
    const confirmed = await confirmAlert({
      title: `Uninstall ${extension.manifest.title}?`,
      message: "Its preferences are kept.",
      primaryAction: { title: "Uninstall", style: "DESTRUCTIVE" },
    });
    if (!confirmed) return;
    fs.rmSync(extension.dir, { recursive: true, force: true });
    await request("host.reload");
    reload();
    showToast({ title: `Uninstalled ${extension.manifest.title}` });
  };

  const stopDeveloping = async (extension: Extension & { source: string }) => {
    const confirmed = await confirmAlert({
      title: `Take ${extension.manifest.title} out of Heyo?`,
      message: `Its sources in ${extension.source} are not touched; heyo dev puts it back.`,
      primaryAction: { title: "Remove", style: "DESTRUCTIVE" },
    });
    if (!confirmed) return;
    unregister(extension.dir);
    await request("host.reload");
    reload();
    showToast({ title: `Removed ${extension.manifest.title}` });
  };

  const developmentItem = (extension: Extension & { source: string }) => {
    const { manifest } = extension;
    const hasPreferences = !!manifest.preferences?.length || manifest.commands.some((c) => c.preferences?.length);
    return (
      <List.Item
        key={extension.dir}
        title={manifest.title}
        subtitle={extension.source}
        icon={manifest.icon ? path.join(extension.dir, "assets", manifest.icon) : Icon.Hammer}
        keywords={[manifest.name, "development"]}
        accessories={[{ tag: { value: "Development", color: Color.Blue }, tooltip: "Built by heyo dev; not updated from the store" }]}
        actions={
          <ActionPanel>
            {hasPreferences && <Action.Push title="Configure" icon={Icon.Gear} target={<ConfigureExtension extension={extension} />} />}
            <Action.Open title="Open Source Folder" target={extension.source} />
            <Action
              title="Stop Developing"
              icon={Icon.Trash}
              style="destructive"
              shortcut={{ modifiers: ["ctrl"], key: "x" }}
              onAction={() => stopDeveloping(extension)}
            />
          </ActionPanel>
        }
      />
    );
  };

  const pending = [...(updates?.values() ?? [])];
  const item = (extension: Extension) => {
    const { manifest } = extension;
    const commands = manifest.commands.filter((c) => c.mode !== "menu-bar").length;
    const hasPreferences = !!manifest.preferences?.length || manifest.commands.some((c) => c.preferences?.length);
    const available = updates?.get(extension.dir);
    return (
      <List.Item
        key={extension.dir}
        title={manifest.title}
        subtitle={manifest.description}
        icon={manifest.icon ? path.join(extension.dir, "assets", manifest.icon) : Icon.Box}
        keywords={[manifest.name]}
        accessories={[
          ...(available ? [{ tag: { value: "Update available", color: Color.Orange } }] : []),
          { text: `${commands} command${commands === 1 ? "" : "s"}` },
        ]}
        actions={
          <ActionPanel>
            <ActionPanel.Section>
              {available && <Action title="Update Extension" icon={Icon.Download} onAction={() => update([available])} />}
              {hasPreferences && (
                <Action.Push title="Configure" icon={Icon.Gear} target={<ConfigureExtension extension={extension} />} />
              )}
              <Action.Open title="Open Folder" target={extension.dir} />
            </ActionPanel.Section>
            <ActionPanel.Section>
              {pending.length > 0 && (
                <Action
                  title={`Update All Extensions (${pending.length})`}
                  icon={Icon.Download}
                  shortcut={{ modifiers: ["cmd", "shift"], key: "u" }}
                  onAction={() => update(pending)}
                />
              )}
              <Action
                title="Check for Updates"
                icon={Icon.ArrowClockwise}
                shortcut={{ modifiers: ["cmd"], key: "r" }}
                onAction={() => {
                  setUpdates(undefined);
                  check();
                }}
              />
              <Action
                title="Uninstall"
                icon={Icon.Trash}
                style="destructive"
                shortcut={{ modifiers: ["ctrl"], key: "x" }}
                onAction={() => uninstall(extension)}
              />
            </ActionPanel.Section>
          </ActionPanel>
        }
      />
    );
  };

  const outdated = extensions.filter((e) => updates?.has(e.dir));
  const current = extensions.filter((e) => !updates?.has(e.dir));
  return (
    <List isLoading={updates === undefined} searchBarPlaceholder="Search installed extensions…">
      <List.EmptyView
        icon={Icon.Box}
        title="No extensions installed"
        description="Run “Search Raycast Store” to install one."
      />
      {developed.length > 0 && <List.Section title="In Development">{developed.map(developmentItem)}</List.Section>}
      {outdated.length > 0 && <List.Section title="Updates Available">{outdated.map(item)}</List.Section>}
      <List.Section title={outdated.length > 0 ? "Up to Date" : developed.length > 0 ? "Installed" : undefined}>{current.map(item)}</List.Section>
    </List>
  );
}
