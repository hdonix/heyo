import { expect, test } from "vitest";

import { reconcile, startTyping, typed, type Typing } from "./typing";

/// An extension with a controlled field: it handles the changes it is sent
/// one at a time, as late as the test likes, and each time answers with its
/// value and the number of the change it handled.
function extension(transform: (text: string) => string = (text) => text, initial = "") {
  let value = initial;
  let ack = 0;
  const inbox: { text: string; seq: number }[] = [];
  return {
    send: (field: Typing) => inbox.push({ text: field.text, seq: field.seq }),
    /// Handles the oldest waiting change and returns the render that follows.
    handle() {
      const change = inbox.shift()!;
      value = transform(change.text);
      ack = change.seq;
      return { value, ack };
    },
    /// The extension changes its value on its own.
    set(text: string) {
      value = text;
      return { value, ack };
    },
  };
}

function type(field: Typing, text: string, to: ReturnType<typeof extension>) {
  const next = typed(field, text);
  to.send(next);
  return next;
}

const arrive = (field: Typing, render: { value: string; ack: number }, composing = false) => reconcile(field, render.value, render.ack, composing);

test("fast typing is never overwritten by the echoes of earlier keystrokes", () => {
  const ext = extension();
  let field = startTyping("", 0);
  field = type(field, "h", ext);
  field = type(field, "he", ext);
  field = type(field, "hel", ext);
  // The echo of "h" arrives after "hel" was typed.
  field = arrive(field, ext.handle());
  expect(field.text).toBe("hel");
  field = type(field, "hell", ext);
  field = arrive(field, ext.handle());
  field = arrive(field, ext.handle());
  expect(field.text).toBe("hell");
  field = arrive(field, ext.handle());
  expect(field).toEqual({ text: "hell", seq: 4, base: "hell" });
});

test("an echo of text that was typed, deleted and typed again changes nothing", () => {
  const ext = extension();
  let field = startTyping("", 0);
  field = type(field, "a", ext);
  field = type(field, "", ext);
  field = type(field, "b", ext);
  for (let i = 0; i < 3; i++) {
    field = arrive(field, ext.handle());
    expect(field.text).toBe("b");
  }
});

test("the extension clearing the field clears it", () => {
  const ext = extension();
  let field = startTyping("", 0);
  field = type(field, "milk", ext);
  field = arrive(field, ext.handle());
  field = arrive(field, ext.set(""));
  expect(field.text).toBe("");
  // And typing goes on from there.
  field = type(field, "e", ext);
  field = arrive(field, ext.handle());
  expect(field.text).toBe("e");
});

test("a field cleared while keys are still on their way keeps what is typed after", () => {
  const ext = extension();
  let field = startTyping("", 0);
  field = type(field, "a", ext);
  field = arrive(field, ext.handle());
  field = type(field, "ab", ext);
  // The extension clears before it has seen "ab".
  field = arrive(field, ext.set(""));
  expect(field.text).toBe("ab");
  field = arrive(field, ext.handle());
  expect(field.text).toBe("ab");
});

test("the extension rewriting what was typed is followed, once it has seen all of it", () => {
  const ext = extension((text) => text.toUpperCase());
  let field = startTyping("", 0);
  field = type(field, "a", ext);
  field = type(field, "ab", ext);
  // "A" answers the first key only: taking it would drop the "b".
  field = arrive(field, ext.handle());
  expect(field.text).toBe("ab");
  field = arrive(field, ext.handle());
  expect(field.text).toBe("AB");
  field = type(field, "ABc", ext);
  field = arrive(field, ext.handle());
  expect(field.text).toBe("ABC");
});

test("an extension that has not yet moved does not take the text back", () => {
  // It handled the change but sets its value later (a debounce, a request).
  let field = startTyping("", 0);
  field = typed(field, "a");
  field = reconcile(field, "", 1, false);
  expect(field.text).toBe("a");
  field = reconcile(field, "A", 1, false);
  expect(field.text).toBe("A");
});

test("nothing arrives in the middle of a composition", () => {
  const ext = extension((text) => text.toUpperCase());
  let field = startTyping("", 0);
  field = type(field, "n", ext);
  field = arrive(field, ext.handle(), true);
  expect(field.text).toBe("n");
  field = arrive(field, ext.set("something else"), true);
  expect(field.text).toBe("n");
  field = type(field, "ni", ext);
  field = arrive(field, ext.handle(), true);
  expect(field.text).toBe("ni");
  // The composition ends: the committed text is typed, then the answer comes.
  field = type(field, "你", ext);
  field = arrive(field, ext.handle());
  expect(field.text).toBe("你");
});

test("what was held back during a composition is taken when it ends", () => {
  let field = startTyping("abc", 3);
  field = reconcile(field, "", 3, true);
  expect(field.text).toBe("abc");
  field = reconcile(field, "", 3, false);
  expect(field.text).toBe("");
});

test("a field that starts out behind the extension's value takes it", () => {
  // A list whose `searchText` is set from the start, or changed while
  // another view covered it.
  expect(reconcile(startTyping("", 0), "preset", 0, false).text).toBe("preset");
  expect(reconcile(startTyping("old", 4), "new", 4, false).text).toBe("new");
});

test("a field that is not controlled is left alone", () => {
  const field = typed(startTyping("", undefined), "a");
  expect(reconcile(field, undefined, undefined, false)).toBe(field);
});

test("a field come back to counts on from the runtime's number", () => {
  const field = typed(startTyping("ab", 7), "abc");
  expect(field.seq).toBe(8);
  // A render from before the key was handled.
  expect(reconcile(field, "x", 7, false).text).toBe("abc");
});
