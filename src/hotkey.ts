import type { Hotkey } from './api';

const MODIFIER_CODE = /^(Control|Shift|Alt|Meta|OS)(Left|Right)?$/;

export function hotkeyFromEvent(event: KeyboardEvent): Hotkey | null {
  if (MODIFIER_CODE.test(event.code) || !event.code) return null;
  return {
    ctrl: event.ctrlKey,
    alt: event.altKey,
    shift: event.shiftKey,
    meta: event.metaKey,
    key: event.code,
  };
}
