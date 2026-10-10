import { describe, expect, it } from "vitest";

import { classifyModelAddress } from "./modelAddress";

const here = { where: "this-computer" };
const elsewhere = (host: string, encrypted: boolean) => ({ where: "elsewhere", host, encrypted });

describe("where the model address points", () => {
  it("knows this computer by every name loopback has", () => {
    for (const address of [
      "http://127.0.0.1:8000/v1",
      "http://127.0.0.1",
      "http://127.8.9.10:11434/v1",
      "http://localhost:1234/v1",
      "http://LOCALHOST:1234/v1",
      "http://localhost.:1234/v1",
      "http://ollama.localhost:11434",
      "https://localhost/v1",
      "http://[::1]:8000/v1",
      "http://[0:0:0:0:0:0:0:1]:8000/v1",
      "http://[::ffff:127.0.0.1]:8000",
      // Other spellings of 127.0.0.1.
      "http://127.1:8000",
      "http://0x7f.0.0.1:8000",
      "http://2130706433:8000",
      "  http://127.0.0.1:8000/v1  ",
    ]) {
      expect(classifyModelAddress(address), address).toEqual(here);
    }
  });

  it("calls a private address another machine", () => {
    expect(classifyModelAddress("http://192.168.1.20:8000/v1")).toEqual(elsewhere("192.168.1.20:8000", false));
    expect(classifyModelAddress("http://10.0.0.5:11434")).toEqual(elsewhere("10.0.0.5:11434", false));
    expect(classifyModelAddress("http://172.16.4.1/v1")).toEqual(elsewhere("172.16.4.1", false));
    expect(classifyModelAddress("http://[fd00::1]:8000")).toEqual(elsewhere("[fd00::1]:8000", false));
    expect(classifyModelAddress("http://gpu-box.local:8000/v1")).toEqual(elsewhere("gpu-box.local:8000", false));
    expect(classifyModelAddress("http://0.0.0.0:8000")).toEqual(elsewhere("0.0.0.0:8000", false));
    expect(classifyModelAddress("http://128.0.0.1:8000")).toEqual(elsewhere("128.0.0.1:8000", false));
  });

  it("tells an encrypted connection from a plain one", () => {
    expect(classifyModelAddress("https://api.example.com/v1")).toEqual(elsewhere("api.example.com", true));
    expect(classifyModelAddress("HTTP://api.example.com/v1")).toEqual(elsewhere("api.example.com", false));
  });

  it("is not fooled by a loopback name that is only part of the address", () => {
    // The host is what follows the "@".
    expect(classifyModelAddress("http://127.0.0.1@evil.example/v1")).toEqual(elsewhere("evil.example", false));
    expect(classifyModelAddress("http://localhost:pw@evil.example/v1")).toEqual(elsewhere("evil.example", false));
    expect(classifyModelAddress("https://localhost.evil.example/v1")).toEqual(elsewhere("localhost.evil.example", true));
    expect(classifyModelAddress("https://127.0.0.1.evil.example/v1")).toEqual(elsewhere("127.0.0.1.evil.example", true));
    expect(classifyModelAddress("http://notlocalhost:8000")).toEqual(elsewhere("notlocalhost:8000", false));
    expect(classifyModelAddress("http://evil.example/?host=127.0.0.1")).toEqual(elsewhere("evil.example", false));
    expect(classifyModelAddress("http://evil.example#@localhost")).toEqual(elsewhere("evil.example", false));
    expect(classifyModelAddress("http://[::ffff:8.8.8.8]:8000")).toEqual(elsewhere("[::ffff:808:808]:8000", false));
  });

  it("does not guess at what is not an http address", () => {
    for (const address of ["", "localhost:8000", "127.0.0.1", "file:///etc/passwd", "ftp://localhost/x", "http://", "not a url"]) {
      expect(classifyModelAddress(address), address).toEqual({ where: "unknown" });
    }
  });
});
