import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";

interface ScanEvent {
  state: "scanning" | "done" | "error" | "idle";
  folder: string;
  added: number;
  error: string | null;
}

export interface ScanStatus {
  text: string;
  error?: boolean;
}

/**
 * Progress of the background library scans, as `library-scan` events from the backend.
 * Each event is handed to `onStatus` (so it replaces whatever notice is showing);
 * `onIdle` runs when a batch of scans ends.
 */
export function useLibraryScan(onStatus: (status: ScanStatus) => void, onIdle: () => void): { busy: boolean } {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  const idle = useRef(onIdle);
  idle.current = onIdle;
  const report = useRef(onStatus);
  report.current = onStatus;
  const setStatus = (status: ScanStatus) => report.current(status);
  const tr = useRef(t);
  tr.current = t;

  useEffect(() => {
    const off = listen<ScanEvent>("library-scan", (e) => {
      const event = e.payload;
      if (event.state === "scanning") {
        setBusy(true);
        setStatus({ text: tr.current("library.folders.scanning", { folder: event.folder }) });
        return;
      }
      if (event.state === "done") {
        setBusy(true);
        setStatus({ text: tr.current("library.folders.scanned", { folder: event.folder, count: event.added }) });
        return;
      }
      if (event.state === "error") {
        setBusy(true);
        setStatus({
          text: tr.current("library.folders.scanError", { folder: event.folder, error: event.error }),
          error: true,
        });
        return;
      }
      setBusy(false);
      setStatus({ text: tr.current("library.folders.scanFinished", { count: event.added }) });
      idle.current();
    });
    return () => { off.then((unlisten) => unlisten()); };
  }, []);

  return { busy };
}
