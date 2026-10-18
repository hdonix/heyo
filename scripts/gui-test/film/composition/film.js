// The film's motion, from window.FILM (data.js, written by edit.py): the
// camera over the plate, the punches on the beat, the captions, the sweeps
// between scenes and the two cards. Everything is placed by time on one
// paused GSAP timeline, which HyperFrames seeks picture by picture.
(() => {
  const F = window.FILM;
  const B = F.beat;
  const tl = gsap.timeline({ paused: true });
  const $ = (selector, root = document) => root.querySelector(selector);
  const make = (tag, className, text) => {
    const element = document.createElement(tag);
    if (className) element.className = className;
    if (text !== undefined) element.textContent = text;
    return element;
  };
  // Words that rise into place: each in a mask of its own.
  const masked = (parent, text, extra) => {
    const outer = make("span", "w");
    const inner = make("span", "wi" + (extra ? " " + extra : ""), text);
    outer.appendChild(inner);
    parent.appendChild(outer);
    return inner;
  };

  // --- The camera ---------------------------------------------------------
  // A place is the middle of the part of the screen shown and its width;
  // between two places the middle moves with the ease named and the width
  // changes by the same ease in proportion (a zoom at an even pace).
  const cam = $("#cam");
  const punch = $("#punch");
  const eases = {};
  const easeOf = (name) => (eases[name] = eases[name] || gsap.parseEase(name || "power2.inOut"));
  const placeAt = (t) => {
    const shot = F.cameras.find((c) => t >= c.from && t < c.to) || F.cameras[F.cameras.length - 1];
    const keys = shot.keys;
    let before = keys[0];
    for (const after of keys) {
      if (after.t > t) {
        if (after.t === before.t) return after;
        const x = easeOf(after.ease)((t - before.t) / (after.t - before.t));
        return {
          x: before.x + (after.x - before.x) * x,
          y: before.y + (after.y - before.y) * x,
          w: Math.exp(Math.log(before.w) + (Math.log(after.w) - Math.log(before.w)) * x),
        };
      }
      before = after;
    }
    return before;
  };
  const shoot = (t) => {
    const place = placeAt(t);
    const w = Math.min(place.w, 3840);
    const h = (w * 9) / 16;
    const x = Math.min(Math.max(place.x - w / 2, 0), 3840 - w);
    const y = Math.min(Math.max(place.y - h / 2, 0), 2160 - h);
    const s = 1920 / w;
    cam.style.transform = `translate(${-x * s}px, ${-y * s}px) scale(${s})`;
    // A punch: the picture a little larger on the beat, settling at once.
    let p = 1;
    for (const at of F.punches) {
      if (t >= at && t < at + 0.6) p = 1 + 0.035 * Math.exp(-(t - at) / 0.11);
    }
    punch.style.transform = `scale(${p})`;
  };
  const clock = { t: 0 };
  tl.to(clock, { t: F.duration, duration: F.duration, ease: "none", onUpdate: () => shoot(clock.t) }, 0);
  shoot(0);

  // Where each window went, flashed on the beat it went there on.
  for (const f of F.flashes) {
    const box = make("div", "snap");
    Object.assign(box.style, { left: `${f.x}px`, top: `${f.y}px`, width: `${f.w}px`, height: `${f.h}px` });
    cam.appendChild(box);
    tl.set(box, { opacity: 0 }, 0);
    tl.fromTo(box, { opacity: 1, scale: 1.03 }, { opacity: 0, scale: 1, duration: 0.45, ease: "power2.in", immediateRender: false }, f.t);
  }

  // --- Captions -----------------------------------------------------------
  const captions = $("#captions");
  F.captions.forEach((c, index) => {
    const box = make("div", "cap " + c.place);
    box.appendChild(make("div", "scrim"));
    const kicker = make("div", "kicker");
    const bar = make("span", "bar");
    const label = make("span", "text", c.kicker);
    kicker.append(bar, label);
    const line = make("div", "headline");
    if (c.kicker) box.append(kicker);
    box.append(line);
    captions.appendChild(box);
    const previous = F.captions[index - 1];
    const next = F.captions[index + 1];
    // The kicker stays when the next caption has the same one.
    const keepsKicker = previous && previous.kicker === c.kicker && Math.abs(previous.to - c.from) < 0.6;
    const passesKicker = next && next.kicker === c.kicker && Math.abs(next.from - c.to) < 0.6;

    tl.set(box, { visibility: "visible" }, c.from);
    tl.set(box, { visibility: "hidden" }, c.to);
    const scrim = $(".scrim", box);
    if (keepsKicker || !c.kicker) {
      if (!c.kicker) tl.fromTo(scrim, { opacity: 0 }, { opacity: 1, duration: 0.35, ease: "power1.out", immediateRender: false }, c.from);
    }
    if (keepsKicker) {
      tl.set(scrim, { opacity: 1 }, c.from);
    } else {
      tl.fromTo(scrim, { opacity: 0 }, { opacity: 1, duration: 0.35, ease: "power1.out", immediateRender: false }, c.from);
      tl.fromTo(bar, { scaleX: 0 }, { scaleX: 1, duration: 0.3, ease: "power3.out", immediateRender: false }, c.from + 0.05);
      tl.fromTo(label, { clipPath: "inset(0 100% 0 0)", x: -12 }, { clipPath: "inset(0 0% 0 0)", x: 0, duration: 0.45, ease: "power3.out", immediateRender: false }, c.from + 0.12);
    }
    if (!passesKicker) {
      tl.to(scrim, { opacity: 0, duration: 0.3, ease: "power1.in" }, c.to - 0.3);
      tl.to(kicker, { opacity: 0, y: -10, duration: 0.25, ease: "power2.in" }, c.to - 0.3);
    }
    const words = c.words.map((w) => {
      const inner = masked(line, w.text, w.accent ? "accent" : "");
      if (w.accent) inner.insertBefore(make("span", "hl"), inner.firstChild);
      return { w, inner };
    });
    for (const { w, inner } of words) {
      // In from below the mask, on its beat.
      tl.fromTo(inner, { yPercent: 118, rotation: 5 }, { yPercent: 0, rotation: 0, duration: 0.5, ease: "expo.out", immediateRender: false }, w.t);
      if (w.accent) {
        const hl = $(".hl", inner);
        tl.set(hl, { scaleX: 0 }, 0);
        tl.fromTo(hl, { scaleX: 0 }, { scaleX: 1, duration: 0.32, ease: "power3.out", immediateRender: false }, w.t + 0.12);
        tl.fromTo(inner, { scale: 1.16 }, { scale: 1, duration: 0.45, ease: "back.out(3)", immediateRender: false }, w.t + 0.12);
      }
    }
    tl.set(words.map((x) => x.inner), { yPercent: 118 }, 0);
    tl.to(words.map((x) => x.inner), { yPercent: -118, duration: 0.28, ease: "power2.in", stagger: 0.025 }, c.to - 0.32);
  });

  // --- The sweeps between scenes --------------------------------------------
  const wipes = $("#wipes");
  for (const w of F.wipes) {
    const box = make("div", "wipe");
    const back = make("div", "panel back");
    const front = make("div", "panel front");
    const word = make("div", "word", w.word);
    box.append(back, front, word);
    wipes.appendChild(box);
    const t = w.t;
    tl.set([back, front], { xPercent: 120 }, 0);
    tl.set(word, { opacity: 0 }, 0);
    tl.set(box, { visibility: "visible" }, t - 0.5);
    tl.set(box, { visibility: "hidden" }, t + 0.62);
    tl.fromTo(back, { xPercent: 120 }, { xPercent: 0, duration: 0.3, ease: "power3.in", immediateRender: false }, t - 0.5);
    tl.fromTo(front, { xPercent: 120 }, { xPercent: 0, duration: 0.3, ease: "power3.in", immediateRender: false }, t - 0.44);
    tl.fromTo(word, { opacity: 0, scale: 1.25, letterSpacing: "0.04em" }, { opacity: 1, scale: 1, letterSpacing: "-0.045em", duration: 0.32, ease: "expo.out", immediateRender: false }, t - 0.16);
    tl.to(word, { opacity: 0, x: -140, duration: 0.24, ease: "power2.in" }, t + 0.3);
    tl.to(front, { xPercent: -120, duration: 0.3, ease: "power3.inOut" }, t + 0.3);
    tl.to(back, { xPercent: -120, duration: 0.3, ease: "power3.inOut" }, t + 0.36);
  }

  // --- The title ------------------------------------------------------------
  const flash = $("#flash");
  const flashAt = (t, strength = 0.85, length = 0.4) =>
    tl.fromTo(flash, { opacity: strength }, { opacity: 0, duration: length, ease: "power2.out", immediateRender: false }, t);
  const card = (selector) => F.cards.find((c) => c.kind === selector);

  {
    const c = card("title");
    const root = $("#title");
    const t = c.from;
    const name = $(".name", root);
    const letters = [..."Heyo"].map((ch) => masked(name, ch));
    const sub = $(".sub", root);
    const subWords = ["The", "keyboard", "launcher", "for", "Linux."].map((w) => masked(sub, w, w === "Linux." ? "accent" : ""));
    const list = $(".list", root);
    const items = ["Dictation", "Windows", "Screenshots", "Clipboard", "Calculator", "Extensions"].map((w) => {
      const item = make("div", "item", w);
      list.appendChild(item);
      return item;
    });
    tl.set(root, { visibility: "visible" }, t);
    tl.set(root, { visibility: "hidden" }, c.to);
    flashAt(t, 0.35, 0.3);
    tl.fromTo($(".glow", root), { scale: 0.6, opacity: 0 }, { scale: 1.15, opacity: 1, duration: 4, ease: "power1.out", immediateRender: false }, t);
    tl.fromTo($(".icon", root), { scale: 0.3, rotation: -14, opacity: 0 }, { scale: 1, rotation: 0, opacity: 1, duration: 0.7, ease: "back.out(2.2)", immediateRender: false }, t);
    tl.set(letters, { yPercent: 115 }, 0);
    tl.fromTo(letters, { yPercent: 115 }, { yPercent: 0, duration: 0.55, ease: "expo.out", stagger: B / 4, immediateRender: false }, t + B);
    tl.set(subWords, { yPercent: 120 }, 0);
    tl.fromTo(subWords, { yPercent: 120 }, { yPercent: 0, duration: 0.5, ease: "expo.out", stagger: B / 2, immediateRender: false }, t + 3 * B);
    tl.set(items, { opacity: 0 }, 0);
    tl.fromTo(items, { opacity: 0, y: 26, scale: 0.9 }, { opacity: 1, y: 0, scale: 1, duration: 0.35, ease: "back.out(2)", stagger: B / 2, immediateRender: false }, t + 4.5 * B);
    // Into the drop: the card thrown towards the camera.
    tl.to($(".lockup", root), { scale: 1.6, opacity: 0, duration: B / 2, ease: "power3.in" }, c.to - B / 2);
    tl.to([sub, list], { scale: 1.3, opacity: 0, duration: B / 2, ease: "power3.in" }, c.to - B / 2);
    tl.to(root, { opacity: 0, duration: 0.12, ease: "none" }, c.to - 0.12);
    flashAt(c.to, 0.5, 0.4);
  }

  // --- The end card ---------------------------------------------------------
  {
    const c = card("end");
    const root = $("#end");
    const t = c.from;
    const name = $(".name", root);
    const letters = [..."Heyo"].map((ch) => masked(name, ch));
    const sub = $(".sub", root);
    const subWords = ["Free", "and", "open", "source"].map((w) => masked(sub, w, w === "open" || w === "source" ? "accent" : ""));
    const url = $(".url", root);
    const typed = $(".typed", url);
    const caret = $(".caret", url);
    const address = "github.com/hdonix/heyo";
    tl.set(root, { visibility: "visible", opacity: 1 }, t);
    flashAt(t, 0.55, 0.5);
    tl.fromTo($(".glow", root), { scale: 1.6, opacity: 0 }, { scale: 1, opacity: 1, duration: 1.2, ease: "power2.out", immediateRender: false }, t);
    tl.fromTo($(".lockup", root), { scale: 1.35, opacity: 0 }, { scale: 1, opacity: 1, duration: 0.7, ease: "expo.out", immediateRender: false }, t);
    tl.set(letters, { yPercent: 0 }, t);
    tl.set(subWords, { yPercent: 120 }, 0);
    tl.fromTo(subWords, { yPercent: 120 }, { yPercent: 0, duration: 0.5, ease: "expo.out", stagger: B / 2, immediateRender: false }, t + 2 * B);
    tl.set(url, { opacity: 0 }, 0);
    tl.fromTo(url, { opacity: 0, scaleX: 0.2, scaleY: 0.6 }, { opacity: 1, scaleX: 1, scaleY: 1, duration: 0.45, ease: "back.out(1.8)", immediateRender: false }, t + 4 * B);
    // The address typed, a letter on each sixteenth.
    const typing = { n: 0 };
    tl.fromTo(typing, { n: 0 }, {
      n: address.length, duration: (address.length * B) / 4, ease: "none", immediateRender: false,
      onUpdate: () => { typed.textContent = address.slice(0, Math.round(typing.n)); },
    }, t + 4.5 * B);
    tl.set(typed, { textContent: "" }, 0);
    // The caret blinks on the beat once the address is there.
    const done = t + 4.5 * B + (address.length * B) / 4;
    for (let k = 0; done + k * B < c.to; k++) {
      tl.set(caret, { opacity: 0 }, done + k * B + B / 2);
      tl.set(caret, { opacity: 1 }, done + (k + 1) * B);
    }
    tl.to("#black", { opacity: 1, duration: F.duration - c.to + 0.2, ease: "power1.in" }, c.to - 0.2);
  }

  // Out of black at the start.
  tl.fromTo("#black", { opacity: 1 }, { opacity: 0, duration: 0.15, ease: "none", immediateRender: false }, 0);
  flashAt(0.0, 0.0, 0.01);

  window.__timelines = window.__timelines || {};
  window.__timelines["main"] = tl;
})();
