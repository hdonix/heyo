// The film's motion, from window.FILM (data.js, written by edit.py): the
// camera over the plate, the punches on the beat, the captions, the sweeps
// between scenes and the two cards. Everything is placed by time on one
// paused GSAP timeline, which HyperFrames seeks picture by picture.
(() => {
  const F = window.FILM;
  const B = F.beat;
  const tl = gsap.timeline({ paused: true });
  // A picture is 1/30 s. Something that comes in on a beat starts moving a
  // picture before it, so that the picture on the beat already shows it
  // arriving (the picture at a tween's start shows it not yet moved); a
  // thing switched on a beat is switched half a picture before, so that the
  // nearest picture shows it.
  const PICTURE = 1 / F.rate;
  const rev = (t) => t - PICTURE;
  const on = (t) => t - PICTURE / 2;
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
    for (const [at, amount] of F.punches) {
      if (t >= at && t < at + 0.6) p = 1 + amount * Math.exp(-(t - at) / 0.11);
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
      // the kicker in, between the cut and the first word
      tl.fromTo(bar, { scaleX: 0 }, { scaleX: 1, duration: 0.2, ease: "power3.out", immediateRender: false }, c.from + 0.12);
      tl.fromTo(label, { clipPath: "inset(0 100% 0 0)", x: -12 }, { clipPath: "inset(0 0% 0 0)", x: 0, duration: 0.25, ease: "power3.out", immediateRender: false }, c.from + 0.15);
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
      tl.fromTo(inner, { yPercent: 118, rotation: 5 }, { yPercent: 0, rotation: 0, duration: 0.3, ease: "expo.out", immediateRender: false }, rev(w.t));
      if (w.accent) {
        // the blue behind the word, and the word punched, on its own hit
        const hl = $(".hl", inner);
        tl.set(hl, { scaleX: 0 }, 0);
        tl.fromTo(hl, { scaleX: 0 }, { scaleX: 1, duration: 0.22, ease: "expo.out", immediateRender: false }, rev(w.t));
        tl.fromTo(inner, { scale: 1.3 }, { scale: 1, duration: 0.4, ease: "back.out(3)", immediateRender: false }, rev(w.t));
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
    // In on the title's hit, held (still) until the scene's, out on it.
    const t = w.t;
    const out = rev(w.to);
    tl.set([back, front], { xPercent: 120 }, 0);
    tl.set(word, { opacity: 0 }, 0);
    tl.set(box, { visibility: "visible" }, t - 0.45);
    tl.set(box, { visibility: "hidden" }, out + 0.34);
    tl.fromTo(back, { xPercent: 120 }, { xPercent: 0, duration: 0.25, ease: "power3.in", immediateRender: false }, t - 0.45);
    tl.fromTo(front, { xPercent: 120 }, { xPercent: 0, duration: 0.25, ease: "power3.in", immediateRender: false }, t - 0.4);
    tl.fromTo(word, { opacity: 0, scale: 1.35 }, { opacity: 1, scale: 1, duration: 0.3, ease: "expo.out", immediateRender: false }, rev(t));
    tl.to(word, { opacity: 0, x: -140, duration: 0.14, ease: "expo.out" }, out);
    tl.to(front, { xPercent: -120, duration: 0.25, ease: "expo.out" }, out);
    tl.to(back, { xPercent: -120, duration: 0.25, ease: "expo.out" }, out + 0.03);
  }

  // --- The title ------------------------------------------------------------
  const flash = $("#flash");
  const flashAt = (t, strength = 0.85, length = 0.4) =>
    tl.fromTo(flash, { opacity: strength }, { opacity: 0, duration: length, ease: "power2.out", immediateRender: false }, t);
  const card = (selector) => F.cards.find((c) => c.kind === selector);

  {
    const c = card("title");
    const T = F.title;
    const root = $("#title");
    const name = $(".name", root);
    const letters = [..."Heyo"].map((ch) => masked(name, ch));
    const sub = $(".sub", root);
    const subWords = T.sub_words.map((w) => masked(sub, w, w === "Linux." ? "accent" : ""));
    const list = $(".list", root);
    const items = T.pills_words.map((w) => {
      const item = make("div", "item", w);
      list.appendChild(item);
      return item;
    });
    tl.set(root, { visibility: "visible" }, c.from);
    tl.set(root, { visibility: "hidden" }, c.to);
    flashAt(c.from, 0.12, 0.15);
    tl.fromTo($(".glow", root), { scale: 0.6, opacity: 0.2 }, { scale: 1.15, opacity: 1, duration: 4, ease: "power1.out", immediateRender: false }, c.from);
    // Each thing here appears whole on its beat and settles after it.
    tl.set($(".icon", root), { opacity: 0 }, 0);
    tl.set($(".icon", root), { opacity: 1 }, on(T.icon));
    tl.fromTo($(".icon", root), { scale: 0.55, rotation: -14 }, { scale: 1, rotation: 0, duration: 0.45, ease: "back.out(2.2)", immediateRender: false }, on(T.icon));
    tl.set(letters, { yPercent: 115 }, 0);
    letters.forEach((letter, k) => tl.fromTo(letter, { yPercent: 115 }, { yPercent: 0, duration: 0.25, ease: "expo.out", immediateRender: false }, rev(T.name[k])));
    tl.set(subWords, { yPercent: 120 }, 0);
    subWords.forEach((word, k) => tl.fromTo(word, { yPercent: 120 }, { yPercent: 0, duration: 0.3, ease: "expo.out", immediateRender: false }, rev(T.sub[k])));
    tl.set(items, { opacity: 0 }, 0);
    items.forEach((item, k) => {
      tl.set(item, { opacity: 1 }, on(T.pills[k]));
      tl.fromTo(item, { y: 26, scale: 0.75 }, { y: 0, scale: 1, duration: 0.3, ease: "back.out(2.5)", immediateRender: false }, on(T.pills[k]));
    });
    // Into the drop: the card struck on the beat, then thrown towards the
    // camera.
    tl.set(root, { scale: 1.08 }, on(T.throw));
    tl.to($(".lockup", root), { scale: 1.7, opacity: 0, duration: c.to - T.throw, ease: "power3.in" }, T.throw);
    tl.to([sub, list], { scale: 1.35, opacity: 0, duration: c.to - T.throw, ease: "power3.in" }, T.throw);
    tl.to(root, { opacity: 0, duration: 0.1, ease: "none" }, c.to - 0.1);
    flashAt(c.to, 0.2, 0.2);
  }

  // --- The end card ---------------------------------------------------------
  {
    const c = card("end");
    const E = F.end;
    const root = $("#end");
    const name = $(".name", root);
    [..."Heyo"].forEach((ch) => masked(name, ch));
    const sub = $(".sub", root);
    const subWords = ["Free", "and", "open", "source"].map((w) => masked(sub, w, w === "open" || w === "source" ? "accent" : ""));
    const url = $(".url", root);
    const typed = $(".typed", url);
    const caret = $(".caret", url);
    tl.set(root, { visibility: "visible", opacity: 1 }, c.from);
    flashAt(c.from, 0.2, 0.25);
    tl.fromTo($(".glow", root), { scale: 1.8, opacity: 0.3 }, { scale: 1, opacity: 1, duration: 1.0, ease: "power2.out", immediateRender: false }, c.from);
    // The name slammed in, on the big hit.
    tl.set($(".lockup", root), { opacity: 0 }, 0);
    tl.set($(".lockup", root), { opacity: 1 }, on(E.slam));
    tl.fromTo($(".lockup", root), { scale: 1.45 }, { scale: 1, duration: 0.45, ease: "expo.out", immediateRender: false }, on(E.slam));
    // The line, all of it on its own hit.
    tl.set(subWords, { yPercent: 120 }, 0);
    tl.fromTo(subWords, { yPercent: 120 }, { yPercent: 0, duration: 0.3, ease: "expo.out", immediateRender: false }, rev(E.line));
    tl.set(url, { opacity: 0 }, 0);
    tl.set(url, { opacity: 1 }, on(E.url));
    tl.fromTo(url, { scaleX: 0.3, scaleY: 0.7 }, { scaleX: 1, scaleY: 1, duration: 0.2, ease: "expo.out", immediateRender: false }, on(E.url));
    // The address typed, a letter on each of its ticks.
    tl.set(typed, { textContent: "" }, 0);
    E.letters.forEach((at, k) => tl.set(typed, { textContent: E.address.slice(0, k + 1) }, on(at)));
    // The caret blinks on the hats: off on the off-beat, on on the beat.
    E.blinks.forEach((at) => tl.set(caret, { opacity: Math.abs(((at / F.beat) % 1) - 0.5) < 0.25 ? 0 : 1 }, on(at)));
    // The last chord: the name and the glow struck once more.
    tl.fromTo($(".lockup", root), { scale: 1.06 }, { scale: 1, duration: 0.6, ease: "power2.out", immediateRender: false }, on(E.chord));
    tl.fromTo($(".glow", root), { opacity: 1.0, scale: 1.25 }, { opacity: 0.8, scale: 1, duration: 1.2, ease: "power2.out", immediateRender: false }, on(E.chord));
    // And out, with the chord's reverb.
    tl.fromTo("#black", { opacity: 0 }, { opacity: 1, duration: E.fade[1] - E.fade[0], ease: "power1.in", immediateRender: false }, E.fade[0]);
  }

  // Out of black, with the swell, to the palette's beat.
  tl.fromTo("#black", { opacity: 1 }, { opacity: 0, duration: F.title.palette * 0.8, ease: "power2.in", immediateRender: false }, 0);

  window.__timelines = window.__timelines || {};
  window.__timelines["main"] = tl;
})();
