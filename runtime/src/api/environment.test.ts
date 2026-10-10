import { afterEach, expect, test, vi } from "vitest";

// The protocol takes over stdout when loaded; these tests only read values.
vi.mock("../protocol", () => ({ request: vi.fn(), send: vi.fn(), on: vi.fn() }));

import { environment, setAppearance } from "./index";

afterEach(() => setAppearance(true));

test("the appearance is what the host last said", () => {
  setAppearance(false);
  expect(environment.appearance).toBe("light");
  expect(environment.theme).toBe("light");
  setAppearance(true);
  expect(environment.appearance).toBe("dark");
});
