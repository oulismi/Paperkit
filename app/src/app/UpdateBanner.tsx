// Checks GitHub for a newer PaperKit (quietly at launch, or when asked from the
// tray) and offers to install it. Updates are signed; the updater refuses any
// file not signed with PaperKit's release key.
import { useEffect, useRef, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { listen } from "@tauri-apps/api/event";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { errorMessage, modelStatus } from "../lib/tauri";

type State =
  | { kind: "hidden" }
  | { kind: "checking" }
  | { kind: "available"; update: Update }
  | { kind: "installing"; done: number; total: number | null }
  | { kind: "info"; text: string; error?: boolean };

const mb = (bytes: number) => `${(bytes / 1_000_000).toFixed(1)} MB`;

export function UpdateBanner({ enabled }: { enabled: boolean }) {
  const [state, setState] = useState<State>({ kind: "hidden" });
  const checked = useRef(false);

  async function run(manual: boolean) {
    // The offline edition carries the AI model inside; updating it with the
    // standard package would drop the model, so it updates by new installer.
    if ((await modelStatus().catch(() => null))?.state === "bundled") {
      if (manual) setState({ kind: "info", text: "This is the offline edition: get new versions as a new offline installer from the website." });
      return;
    }
    if (manual) setState({ kind: "checking" });
    try {
      const update = await check();
      if (update) setState({ kind: "available", update });
      else if (manual) setState({ kind: "info", text: `PaperKit ${await getVersion()} is the latest version.` });
    } catch (e) {
      // Offline or GitHub unreachable: stay quiet unless the user asked.
      if (manual) setState({ kind: "info", text: `Couldn't check for updates: ${errorMessage(e)}`, error: true });
    }
  }

  useEffect(() => {
    if (!enabled || checked.current) return;
    checked.current = true;
    void run(false);
  }, [enabled]);

  useEffect(() => {
    const unlisten = listen("paperkit://check-updates", () => void run(true));
    return () => void unlisten.then((f) => f());
  }, []);

  async function install(update: Update) {
    let done = 0;
    let total: number | null = null;
    setState({ kind: "installing", done, total });
    try {
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") total = event.data.contentLength ?? null;
        if (event.event === "Progress") done += event.data.chunkLength;
        setState({ kind: "installing", done, total });
      });
      await relaunch();
    } catch (e) {
      setState({ kind: "info", text: `The update didn't install: ${errorMessage(e)}`, error: true });
    }
  }

  if (state.kind === "hidden") return null;
  return (
    <div className={`update-banner${state.kind === "info" && state.error ? " err" : ""}`} role="status">
      {state.kind === "checking" && <span>Checking for updates…</span>}
      {state.kind === "available" && (
        <>
          <span>
            <strong>PaperKit {state.update.version}</strong> is available{state.update.body ? `: ${state.update.body.split("\n")[0]}` : "."}
          </span>
          <button type="button" className="btn" style={{ height: 34 }} onClick={() => void install(state.update)}>
            Update & restart
          </button>
          <button type="button" className="btn-ghost solid" onClick={() => setState({ kind: "hidden" })}>
            Later
          </button>
        </>
      )}
      {state.kind === "installing" && (
        <>
          <span>Downloading the update… {state.total ? `${mb(state.done)} of ${mb(state.total)}` : mb(state.done)}</span>
          {state.total && <progress className="cv-bar" style={{ maxWidth: 240 }} max={state.total} value={state.done} />}
        </>
      )}
      {state.kind === "info" && (
        <>
          <span>{state.text}</span>
          <button type="button" className="btn-ghost solid" onClick={() => setState({ kind: "hidden" })}>
            OK
          </button>
        </>
      )}
    </div>
  );
}
