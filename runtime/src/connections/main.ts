//! Heyo's connections runtime: the process connectors run in, apart from
//! the commands of the palette and the background, so that a sync never
//! slows them down nor they it. The host starts it when there is an
//! account to sync (`src-tauri/src/connections/shell.rs`); its closing our
//! input is Heyo being done with it.

import { pipes } from "../transport";
import { loader } from "./load";
import { serve } from "./serve";

const host = pipes();
const exit = process.exit.bind(process);

// A connector's mistake is its sync's failure, not the process's end.
process.on("unhandledRejection", (error) => console.error("heyo-connections:", error));
process.on("uncaughtException", (error) => console.error("heyo-connections:", error));

serve(host, loader()).then(() => exit(0));
