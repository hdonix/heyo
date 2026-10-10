// The dictation window: a small pill at the bottom of the screen with a live
// waveform of the microphone, which widens to show the latest words as they
// are recognized. It never takes keyboard focus; clicking it finishes the
// recording.

import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { X } from "lucide-react";
import { event } from "../shared/bindings/events";

type Phase = "loading" | "listening" | "polishing" | "done" | "error";

interface DictationEvent {
  type: string;
  text?: string;
  raw?: string;
  value?: number;
  message?: string;
  warning?: string;
  cancelled?: boolean;
}

/// Bars in the waveform; the newest level is on the right.
const BARS = 22;
const silence = () => Array<number>(BARS).fill(0);

export function DictationWindow() {
  const [phase, setPhase] = useState<Phase>("loading");
  const [committed, setCommitted] = useState("");
  const [partial, setPartial] = useState("");
  const [levels, setLevels] = useState(silence);
  const [note, setNote] = useState("");
  /// The loudest level lately, slowly forgotten.
  const peak = useRef(0.2);

  useEffect(() => {
    const unlisten = listen<DictationEvent>(event.dictation, ({ payload: e }) => {
      switch (e.type) {
        case "start":
          setPhase("loading");
          setCommitted("");
          setPartial("");
          setNote("");
          setLevels(silence());
          break;
        case "ready":
          setPhase("listening");
          break;
        case "level":
          // Drawn as they arrive (about 25 a second). Not batched into
          // animation frames: this window is shown and hidden all the time,
          // and a frame requested while it is hidden may never come, which
          // left the waveform frozen from then on.
          {
            // Scaled to the loudest recent level, so a quiet microphone
            // (a Bluetooth headset, say) still fills the bars when spoken to.
            const value = e.value ?? 0;
            peak.current = Math.max(value, peak.current * 0.995, 0.2);
            const shown = Math.min(1, value / peak.current);
            setLevels((l) => [...l.slice(1 - BARS), shown]);
          }
          break;
        case "partial":
          setPartial(e.text ?? "");
          break;
        case "segment":
          setCommitted((c) => c + (e.text ?? ""));
          setPartial("");
          break;
        case "polishing":
          setPhase("polishing");
          setPartial("");
          break;
        case "result":
          setPhase("done");
          setPartial("");
          if (e.warning) setNote(`Not polished: ${e.warning}`);
          else if (e.cancelled) setNote("Cancelled");
          else if (!e.raw) setNote("Didn't catch anything");
          else if (e.text) setCommitted(e.text);
          break;
        case "error":
          setPhase("error");
          setNote(e.message ?? "Dictation failed");
          break;
      }
    });
    return () => {
      void unlisten.then((off) => off());
    };
  }, []);

  const recording = phase === "loading" || phase === "listening";
  const text = note || committed + partial;

  return (
    <div className="dictation-window">
      <div
        className={`dictation ${phase}`}
        title={recording ? "Click to finish" : undefined}
        onClick={() => recording && invoke("dictation_toggle")}
      >
        <div className="waveform">
          {levels.map((level, i) => (
            // A quiet room still shows a faint line; speech reaches full height.
            <span key={i} style={{ height: `${recording ? Math.max(12, level * 100) : 12}%` }} />
          ))}
        </div>
        {text && (
          <div className="dictation-text">
            {/* Clipped on the left, so the newest words stay visible. */}
            <span>
              {note || committed}
              {!note && <span className="dictation-partial">{partial}</span>}
            </span>
          </div>
        )}
        {recording && (
          <button
            className="icon-button"
            title="Cancel"
            onClick={(e) => {
              e.stopPropagation();
              invoke("dictation_cancel");
            }}
          >
            <X size={12} />
          </button>
        )}
      </div>
    </div>
  );
}
