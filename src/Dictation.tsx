// The dictation window: a small dark pill at the bottom of the screen. While
// it listens it shows the microphone's level and says so; the latest words
// take the place of that unless they are going into the application as they
// are spoken: then they are there to be read. While the text is polished it
// says that, beside a sparkle. It never takes keyboard focus; clicking it
// finishes the recording.

import { useEffect, useRef, useState } from "react";
import { X } from "lucide-react";
import { call, listen, reported } from "./host";

type Phase = "loading" | "listening" | "polishing" | "done" | "error";

/// Levels kept for the waveform: the newest is drawn in the middle and the
/// ones before it on either side, so that the bars swell from the centre.
const KEPT = 3;
const silence = () => Array<number>(KEPT).fill(0);

/// What the pill says of itself when it has no words to show.
const SAYING: Record<Phase, string> = {
  loading: "Starting…",
  listening: "Listening",
  polishing: "Polishing…",
  done: "Done",
  error: "",
};

/// Two stars, for the polishing that an AI model does.
function Sparkle() {
  return (
    <svg className="dictation-sparkle" width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">
      <defs>
        <linearGradient id="dictation-sparkle" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#9bd4ff" />
          <stop offset="1" stopColor="#2f7df6" />
        </linearGradient>
      </defs>
      <path
        fill="url(#dictation-sparkle)"
        d="M9.5 2.5c.5 2.9 1.6 4 4.5 4.5-2.9.5-4 1.6-4.5 4.5-.5-2.9-1.6-4-4.5-4.5 2.9-.5 4-1.6 4.5-4.5Z"
      />
      <path
        fill="url(#dictation-sparkle)"
        d="M4 9.5c.3 1.6.9 2.2 2.5 2.5-1.6.3-2.2.9-2.5 2.5-.3-1.6-.9-2.2-2.5-2.5 1.6-.3 2.2-.9 2.5-2.5Z"
      />
    </svg>
  );
}

export function DictationWindow() {
  const [phase, setPhase] = useState<Phase>("loading");
  const [committed, setCommitted] = useState("");
  const [partial, setPartial] = useState("");
  const [levels, setLevels] = useState(silence);
  const [note, setNote] = useState("");
  /// The words are going into the application as they are spoken.
  const [live, setLive] = useState(false);
  /// The loudest level lately, slowly forgotten.
  const peak = useRef(0.2);

  useEffect(() => {
    return listen("dictation", (e) => {
      switch (e.type) {
        case "start":
          setPhase("loading");
          setCommitted("");
          setPartial("");
          setNote("");
          setLive(false);
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
            setLevels((l) => [...l.slice(1 - KEPT), shown]);
          }
          break;
        case "partial":
          setPartial(e.text);
          break;
        case "segment":
          setCommitted((c) => c + e.text);
          setPartial("");
          break;
        case "writing":
          setLive(e.live);
          break;
        case "polishing":
          setPhase("polishing");
          setPartial("");
          break;
        case "result":
          setPhase("done");
          setPartial("");
          if (e.warning) setNote(`Not polished: ${e.warning}`);
          else if (e.cancelled) setNote(e.kept ? "Cancelled. What was written is still there." : "Cancelled");
          else if (!e.raw) setNote("Didn't catch anything");
          else if (e.text) setCommitted(e.text);
          break;
        case "error":
          setPhase("error");
          setNote(e.message);
          break;
      }
    });
  }, []);

  const recording = phase === "loading" || phase === "listening";
  const text = note || (live ? "" : committed + partial);
  // Older, newer, newest, newer, older.
  const bars = [levels[0], levels[1], levels[2], levels[1], levels[0]];

  return (
    // A press anywhere on the window is said at once, before the click it
    // may become: the host must not take it for a click in the application.
    <div className="dictation-window" onMouseDown={() => call("dictation_pressed").catch(reported())}>
      <div
        className={`dictation ${phase}`}
        title={recording ? "Click to finish" : undefined}
        onClick={() => recording && call("dictation_toggle").catch(reported())}
      >
        {phase === "polishing" ? (
          <Sparkle />
        ) : (
          phase !== "error" && (
            <div className="waveform">
              {bars.map((level, i) => (
                // A quiet room still shows a faint line; speech reaches full height.
                <span key={i} style={{ height: `${recording ? Math.max(18, level * 100) : 18}%` }} />
              ))}
            </div>
          )
        )}
        {text ? (
          <div className="dictation-text">
            {/* Clipped on the left, so the newest words stay visible. */}
            <span>
              {note || committed}
              {!note && <span className="dictation-partial">{partial}</span>}
            </span>
          </div>
        ) : (
          <span className="dictation-saying">{SAYING[phase]}</span>
        )}
        {recording && (
          <button
            className="icon-button"
            title="Cancel"
            onClick={(e) => {
              e.stopPropagation();
              call("dictation_cancel").catch(reported());
            }}
          >
            <X size={11} />
          </button>
        )}
      </div>
    </div>
  );
}
