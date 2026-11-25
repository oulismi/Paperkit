import { useEffect, useState } from "react";
import { Icon } from "../../components/Icon";
import { aiOffer, aiOfferDecline, errorMessage, modelDownload, modelDownloadCancel, modelStatus } from "../../lib/tauri";

type State =
  | { kind: "hidden" }
  | { kind: "ask"; bytes: number }
  | { kind: "downloading"; done: number; total: number }
  | { kind: "ready" }
  | { kind: "failed"; error: string; bytes: number };

const mb = (bytes: number) => `${Math.round(bytes / 1_000_000)} MB`;

// Asked once per app run, so leaving Home and coming back doesn't ask again.
let askedThisRun = false;

/** First-launch card offering the on-device AI model (or downloading it, if the installer was told to). */
export function AiWelcome() {
  const [state, setState] = useState<State>({ kind: "hidden" });

  useEffect(() => {
    if (askedThisRun) return;
    askedThisRun = true;
    Promise.all([aiOffer(), modelStatus()]).then(([offer, status]) => {
      if (!offer.show) return;
      if (offer.startDownload) void download(status.downloadBytes);
      else setState({ kind: "ask", bytes: status.downloadBytes });
    }, () => {});
  }, []);

  async function download(bytes: number) {
    setState({ kind: "downloading", done: 0, total: bytes });
    try {
      await modelDownload((done, total) => setState({ kind: "downloading", done, total }));
      setState({ kind: "ready" });
    } catch (e) {
      setState({ kind: "failed", error: errorMessage(e), bytes });
    }
  }

  if (state.kind === "hidden") return null;

  return (
    <section className="ai-welcome" aria-labelledby="ai-welcome-title">
      <span className="ai-welcome-icon" aria-hidden="true">
        <Icon name="sparkle" size={20} />
      </span>
      <div className="ai-welcome-body">
        <h2 id="ai-welcome-title">
          {state.kind === "ready" ? "On-device AI is ready" : "Find names and addresses too?"}
        </h2>
        {state.kind === "ask" && (
          <p>
            Redact already finds emails, phone numbers, IBANs, ID numbers and dates. With the on-device AI model ({mb(state.bytes)}, one-time
            download) it also finds people's names and addresses in English, French, Dutch and Greek. It runs on this computer; your documents
            never leave it.
          </p>
        )}
        {state.kind === "downloading" && (
          <>
            <p>Downloading the AI model. You can keep working meanwhile.</p>
            <progress className="ai-progress" max={state.total} value={state.done} aria-label="AI model download" />
            <span className="result">
              {mb(state.done)} of {mb(state.total)}
            </span>
          </>
        )}
        {state.kind === "ready" && <p>Redact now also finds names and addresses. Everything stays offline from here on.</p>}
        {state.kind === "failed" && <p className="error">{state.error}</p>}
      </div>
      <div className="ai-welcome-actions">
        {(state.kind === "ask" || state.kind === "failed") && (
          <>
            <button type="button" className="btn" onClick={() => void download(state.bytes)}>
              {state.kind === "failed" ? "Try again" : `Download now (${mb(state.bytes)})`}
            </button>
            <button type="button" className="btn-ghost solid" onClick={() => setState({ kind: "hidden" })}>
              Later
            </button>
            <button
              type="button"
              className="btn-ghost solid"
              title="Don't ask again. Redact will still offer it."
              onClick={() => {
                void aiOfferDecline();
                setState({ kind: "hidden" });
              }}
            >
              No thanks
            </button>
          </>
        )}
        {state.kind === "downloading" && (
          <button type="button" className="btn-ghost solid" onClick={() => void modelDownloadCancel()}>
            Cancel
          </button>
        )}
        {state.kind === "ready" && (
          <button type="button" className="btn-ghost solid" onClick={() => setState({ kind: "hidden" })}>
            Done
          </button>
        )}
      </div>
    </section>
  );
}
