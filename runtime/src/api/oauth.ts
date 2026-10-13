//! OAuth with PKCE, as Raycast extensions use it. Their OAuth apps are
//! registered with Raycast's redirect addresses, which all end in a
//! `raycast://oauth?…` (or `com.raycast:/oauth?…`) link; Heyo is registered
//! on the desktop as the handler for those links and the host passes each one
//! on as an "oauth-callback" message.

import { createHash, randomBytes, randomUUID } from "node:crypto";
import fs from "node:fs";
import path from "node:path";

import { ownsPalette, readJson, requireCurrent, supportPath } from "../context";
import { on, request } from "../protocol";
import { open, showToast, Toast } from "./system";
import { t } from "../../../shared/i18n";

const RedirectMethod = { Web: "web", App: "app", AppURI: "appURI" } as const;
/// How long the user has to finish signing in in the browser.
const TIMEOUT = 5 * 60 * 1000;

interface ClientOptions {
  redirectMethod: string;
  providerName: string;
  providerIcon?: unknown;
  providerId?: string;
  description?: string;
}

interface AuthorizationRequestOptions {
  endpoint: string;
  clientId: string;
  scope: string;
  extraParameters?: Record<string, string>;
}

interface AuthorizationRequest {
  codeChallenge: string;
  codeVerifier: string;
  state: string;
  redirectURI: string;
  toURL(): string;
}

interface TokenSetOptions {
  accessToken: string;
  refreshToken?: string;
  idToken?: string;
  expiresIn?: number;
  scope?: string;
}

/// The shape of a token endpoint's JSON response.
interface TokenResponse {
  access_token: string;
  refresh_token?: string;
  id_token?: string;
  expires_in?: number;
  scope?: string;
}

interface StoredTokens extends TokenSetOptions {
  updatedAt: string;
}

const base64url = (data: Buffer) => data.toString("base64url");

/// Sign-ins waiting for their redirect, by the `id` inside their state.
const waiting = new Map<string, { state: string; resolve(code: string): void; reject(error: Error): void }>();

function stateId(state: string | null): string | undefined {
  if (!state) return undefined;
  for (const decode of [(s: string) => Buffer.from(s, "base64url").toString(), (s: string) => s]) {
    try {
      const id = JSON.parse(decode(state)).id;
      if (typeof id === "string") return id;
    } catch {
      // Not in this encoding.
    }
  }
  return undefined;
}

on("oauth-callback", (message) => {
  let params: URLSearchParams;
  try {
    params = new URL(String(message.url)).searchParams;
  } catch {
    return;
  }
  // The state ties the redirect to the sign-in that asked for it; without a
  // match the link could come from anywhere and is ignored.
  const state = params.get("state");
  const id = stateId(state);
  const pending = id ? waiting.get(id) : undefined;
  if (!id || !pending || pending.state !== state) return;
  waiting.delete(id);
  const code = params.get("code");
  if (code) pending.resolve(code);
  else pending.reject(new Error(params.get("error_description") ?? params.get("error") ?? "The sign-in did not return a code"));
});

export class PKCEClient {
  redirectMethod: string;
  providerName: string;
  providerIcon?: unknown;
  providerId?: string;
  description?: string;
  /// The extension's support directory, where its tokens are kept.
  private directory: string;

  constructor(options: ClientOptions) {
    this.redirectMethod = options.redirectMethod;
    this.providerName = options.providerName;
    this.providerIcon = options.providerIcon;
    this.providerId = options.providerId;
    this.description = options.description;
    this.directory = supportPath(requireCurrent().extension.manifest.name);
  }

  async authorizationRequest(options: AuthorizationRequestOptions): Promise<AuthorizationRequest> {
    const codeVerifier = base64url(randomBytes(48));
    const codeChallenge = base64url(createHash("sha256").update(codeVerifier).digest());
    // Raycast's redirect page expects the state to be base64 JSON and hands it back unchanged.
    const state = base64url(Buffer.from(JSON.stringify({ id: randomUUID(), flavor: "release", providerName: this.providerName })));
    const redirectURI =
      this.redirectMethod === RedirectMethod.App
        ? "raycast://oauth?package_name=Extension"
        : this.redirectMethod === RedirectMethod.AppURI
          ? "com.raycast:/oauth?package_name=Extension"
          : "https://raycast.com/redirect?packageName=Extension";
    const toURL = () => {
      const url = new URL(options.endpoint);
      const params = {
        response_type: "code",
        client_id: options.clientId,
        redirect_uri: redirectURI,
        scope: options.scope,
        state,
        code_challenge: codeChallenge,
        code_challenge_method: "S256",
        ...options.extraParameters,
      };
      for (const [key, value] of Object.entries(params)) url.searchParams.set(key, value);
      return url.toString();
    };
    return { codeChallenge, codeVerifier, state, redirectURI, toURL };
  }

  async authorize(options: AuthorizationRequest | { url: string }): Promise<{ authorizationCode: string }> {
    const url = "url" in options ? options.url : options.toURL();
    const state = "state" in options ? options.state : new URL(url).searchParams.get("state");
    const id = stateId(state);
    if (!state || !id) throw new Error("The sign-in link has no state Heyo can match the redirect with");

    const toast = await showToast({
      style: Toast.Style.Animated,
      title: t("Sign in to {0}", this.providerName),
      message: t("Continue in your browser…"),
    });
    try {
      const code = await new Promise<string>((resolve, reject) => {
        waiting.set(id, { state, resolve, reject });
        setTimeout(() => waiting.delete(id) && reject(new Error("Timed out waiting for the sign-in")), TIMEOUT).unref();
        open(url).catch(reject);
      });
      // The browser has the focus; come back to the command, if it is the
      // one in the palette (a menu bar command signs in without it).
      if (ownsPalette()) await request("host.show").catch(() => {});
      toast.style = Toast.Style.Success;
      toast.title = t("Signed in to {0}", this.providerName);
      toast.message = undefined;
      return { authorizationCode: code };
    } catch (e) {
      waiting.delete(id);
      toast.style = Toast.Style.Failure;
      toast.title = t("Could not sign in to {0}", this.providerName);
      toast.message = e instanceof Error ? e.message : String(e);
      throw e;
    }
  }

  private file() {
    const name = (this.providerId ?? this.providerName).replace(/[^\w.-]+/g, "_");
    return path.join(this.directory, `oauth-${name}.json`);
  }

  async setTokens(options: TokenSetOptions | TokenResponse) {
    const tokens: StoredTokens =
      "access_token" in options
        ? {
            accessToken: options.access_token,
            refreshToken: options.refresh_token,
            idToken: options.id_token,
            expiresIn: options.expires_in,
            scope: options.scope,
            updatedAt: new Date().toISOString(),
          }
        : { ...options, updatedAt: new Date().toISOString() };
    fs.mkdirSync(this.directory, { recursive: true });
    // Tokens are credentials: readable by the user only.
    fs.writeFileSync(this.file(), JSON.stringify(tokens, null, 2), { mode: 0o600 });
    fs.chmodSync(this.file(), 0o600);
  }

  async getTokens() {
    const stored = readJson<StoredTokens | null>(this.file(), null);
    if (!stored?.accessToken) return undefined;
    const updatedAt = new Date(stored.updatedAt);
    return {
      ...stored,
      updatedAt,
      isExpired: () => stored.expiresIn !== undefined && updatedAt.getTime() + stored.expiresIn * 1000 < Date.now(),
    };
  }

  async removeTokens() {
    fs.rmSync(this.file(), { force: true });
  }
}

export const OAuth = { PKCEClient, RedirectMethod };
