//! `Windows`, `Memory`, `ClipboardHistory`, `Files` and `System`: the
//! modules that are nothing but their requests. All there is to say of
//! them is their declaration (`members.ts`).

import { implement } from "./asking";
import { modules } from "./members";

export const Windows = implement(modules.Windows);
export const Memory = implement(modules.Memory);
export const ClipboardHistory = implement(modules.ClipboardHistory);
export const Files = implement(modules.Files);
export const System = implement(modules.System);
