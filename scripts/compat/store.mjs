//! The Raycast Store, as `runtime/src/builtins/store.tsx` asks it: the
//! listing, and an extension's built bundle (a zip). Everything fetched is
//! kept in the cache directory, so a run that stops can go on where it was
//! and the Store is asked once.

import fs from "node:fs";
import path from "node:path";

import { extract } from "./unzip.mjs";

export const API = "https://www.raycast.com/api/v1";
/// Heyo's runtime asks the Store with Node's `fetch` and sets no
/// User-Agent, so the Store sees Node's own: "node". The same here, said
/// outright so that it stays the same whatever does the fetching.
export const USER_AGENT = "node";
/// The most the listing gives in one page.
const PER_PAGE = 200;

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/// The extensions with the most downloads, the most first. Ties keep the
/// Store's own order, so the pick is the same from one run to the next.
export function mostDownloaded(listings, count) {
  const seen = new Set();
  return listings
    .filter((listing) => {
      const key = `${listing.owner?.handle}/${listing.name}`;
      if (seen.has(key)) return false;
      seen.add(key);
      return listing.status === undefined || listing.status === "active";
    })
    .map((listing, index) => ({ listing, index }))
    .sort((a, b) => (b.listing.download_count ?? 0) - (a.listing.download_count ?? 0) || a.index - b.index)
    .slice(0, count)
    .map(({ listing }) => listing);
}

/// What of a listing the report keeps.
export function slim(listing) {
  return {
    name: listing.name,
    title: listing.title,
    description: listing.description,
    owner: listing.owner?.handle,
    author: listing.author?.name,
    downloads: listing.download_count,
    platforms: listing.platforms ?? [],
    categories: listing.categories ?? [],
    commit: listing.commit_sha,
    apiVersion: listing.api_version,
    storeUrl: listing.store_url,
    tools: (listing.tools ?? []).length,
  };
}

export function createStore({ cacheDir, pauseMs = 500, refresh = false, log = () => {} }) {
  let last = 0;
  // One request at a time, with a pause after each: a hundred extensions
  // are not worth a burst.
  let queue = Promise.resolve();
  const polite = (url) => {
    const next = queue.then(async () => {
      const wait = last + pauseMs - Date.now();
      if (wait > 0) await sleep(wait);
      // A connection that drops is tried again, a little later each time.
      for (let attempt = 1; ; attempt++) {
        try {
          return await fetch(url, { headers: { "User-Agent": USER_AGENT }, signal: AbortSignal.timeout(120_000) });
        } catch (e) {
          if (attempt >= 3) throw e;
          await sleep(attempt * 2_000);
        } finally {
          last = Date.now();
        }
      }
    });
    queue = next.catch(() => {});
    return next;
  };

  const getJson = async (url) => {
    const res = await polite(url);
    if (!res.ok) throw new Error(`${res.status} ${res.statusText} for ${url}`);
    return res.json();
  };

  /// The whole public listing, in the Store's order.
  async function listing() {
    const dir = path.join(cacheDir, "listing");
    fs.mkdirSync(dir, { recursive: true });
    const all = [];
    let fetchedAt;
    for (let page = 1; ; page++) {
      const file = path.join(dir, `page-${page}.json`);
      let body;
      if (!refresh && fs.existsSync(file)) {
        body = JSON.parse(fs.readFileSync(file, "utf8"));
      } else {
        log(`listing page ${page}`);
        body = { fetchedAt: new Date().toISOString(), data: (await getJson(`${API}/store_listings?per_page=${PER_PAGE}&page=${page}`)).data ?? [] };
        fs.writeFileSync(file, JSON.stringify(body));
      }
      fetchedAt ??= body.fetchedAt;
      all.push(...body.data);
      if (body.data.length < PER_PAGE) break;
    }
    return { listings: all, fetchedAt };
  }

  /// The directory an extension's bundle is unpacked in, downloading it if
  /// it is not there yet. Named by commit: a new version is another one.
  async function unpacked(entry) {
    const id = `${entry.owner?.handle}__${entry.name}@${(entry.commit_sha ?? "unknown").slice(0, 12)}`.replace(/[^\w.@-]/g, "_");
    const dir = path.join(cacheDir, "extensions", id);
    if (fs.existsSync(path.join(dir, "package.json"))) return dir;
    const zip = path.join(cacheDir, "zips", `${id}.zip`);
    fs.mkdirSync(path.dirname(zip), { recursive: true });
    if (!fs.existsSync(zip)) {
      log(`download ${entry.name}`);
      let res = await polite(entry.download_url);
      if (!res.ok) {
        // The listing's pre-signed URL expires; ask for a fresh one.
        const fresh = await getJson(`${API}/extensions/${entry.owner.handle}/${entry.name}`);
        res = await polite(fresh.download_url);
      }
      if (!res.ok) throw new Error(`download failed: ${res.status} ${res.statusText}`);
      const tmp = `${zip}.${process.pid}.tmp`;
      fs.writeFileSync(tmp, Buffer.from(await res.arrayBuffer()));
      fs.renameSync(tmp, zip);
    }
    const staging = `${dir}.${process.pid}.tmp`;
    fs.rmSync(staging, { recursive: true, force: true });
    fs.mkdirSync(staging, { recursive: true });
    extract(fs.readFileSync(zip), staging);
    fs.renameSync(staging, dir);
    return dir;
  }

  return { listing, unpacked };
}
