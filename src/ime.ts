// Input-method composition. While the user is composing (typing pinyin, kana,
// …), Enter, Escape and Tab belong to the input method: Enter commits the
// candidate, Escape cancels it. Handlers that act on those keys ask here first.

/// True for a key event that the input method is handling.
export function composing(e: { nativeEvent?: KeyboardEvent; isComposing?: boolean; keyCode: number }): boolean {
  // WebKitGTK reports composition keys with keyCode 229; isComposing is not
  // set on every one of them.
  return (e.nativeEvent?.isComposing ?? e.isComposing ?? false) || e.keyCode === 229;
}
