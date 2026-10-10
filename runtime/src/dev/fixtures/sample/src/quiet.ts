import { showHUD } from "@raycast/api";

import { greet } from "./lib/greeting";

export default async function quiet() {
  await showHUD(greet("Heyo"));
}
