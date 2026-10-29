// Commands the menus and keyboard shortcuts send to the panels that own the action.
import { useEffect } from "react";

export type CommandId =
  | "library.addFolder" | "library.search" | "library.completeMetadata"
  | "playlist.open" | "playlist.save" | "playlist.clear"
  | "stream.settings" | "stream.start" | "stream.stop"
  | "categories.settings";

const EVENT = "riars-command";

export function runCommand(id: CommandId) {
  window.dispatchEvent(new CustomEvent<CommandId>(EVENT, { detail: id }));
}

/** Run `handler` when `id` is sent. */
export function useCommand(id: CommandId, handler: () => void) {
  useEffect(() => {
    const listener = (e: Event) => {
      if ((e as CustomEvent<CommandId>).detail === id) handler();
    };
    window.addEventListener(EVENT, listener);
    return () => window.removeEventListener(EVENT, listener);
  }, [id, handler]);
}
