import { useEffect, useState } from "react";
import { Icon } from "../../components/Icon";
import { errorMessage, modelDownload, modelDownloadCancel, modelStatus, type ModelStatus } from "../../lib/tauri";

export type AiRun = { state: "idle" } | { state: "running"; what: string } | { state: "done"; found: number; ms: number } | { state: "failed"; error: string };

interface Props {
  run: AiRun;
  /** Called once the model is available (already installed, or just downloaded). */
  onReady: () => void;
  /** Look for something described in words, e.g. "patient name". */
  onLookFor: (description: string) => void;
}

const mb = (bytes: number) => `${Math.round(bytes / 1_000_000)} MB`;

export function AiCard({ run, onReady, onLookFor }: Props) {
  const [status, setStatus] = useState<ModelStatus | null>(null);
  const [download, setDownload] = useState<{ done: number; total: number } | null>(null);
  const [error, setError] = useState("");
  const [lookFor, setLookFor] = useState("");

  useEffect(() => {
    modelStatus().then((s) => {
      setStatus(s);
      if (s.state !== "missing") onReady();
    }, (e) => setError(errorMessage(e)));
    // Runs once per file: the screen is remounted for each file.
  }, []);

  async function start() {
    if (!status) return;
    setError("");
    setDownload({ done: 0, total: status.downloadBytes });
    try {
      await modelDownload((done, total) => setDownload({ done, total }));
      setStatus({ ...status, state: "downloaded" });
      onReady();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setDownload(null);
    }
  }

  if (!status) return null;

  if (status.state === "missing") {
    return (
      <section className="ai-card" aria-labelledby="ai-title">
        <h3 id="ai-title" className="ai-title">
          <Icon name="sparkle" size={16} /> Find names &amp; addresses with AI
        </h3>
        {download ? (
          <>
            <progress className="ai-progress" max={download.total} value={download.done} aria-label="Model download" />
            <div className="ai-row">
              <span className="result">
                {mb(download.done)} of {mb(download.total)}
              </span>
              <button type="button" className="btn-ghost solid" onClick={() => void modelDownloadCancel()}>
                Cancel
              </button>
            </div>
          </>
        ) : (
          <>
            <p className="result" style={{ lineHeight: 1.5 }}>
              Uses an on-device model ({status.name}). One-time download of {mb(status.downloadBytes)} from Hugging Face; after that it
              works offline and your documents never leave this computer.
            </p>
            <button type="button" className="btn-ghost ai-btn" onClick={start}>
              Download model ({mb(status.downloadBytes)})
            </button>
          </>
        )}
        {error && <p className="error">{error}</p>}
      </section>
    );
  }

  return (
    <section className="ai-card" aria-labelledby="ai-title">
      <h3 id="ai-title" className="ai-title">
        <Icon name="sparkle" size={16} /> On-device AI
      </h3>
      <p className="result" role="status">
        {run.state === "running" && `Looking for ${run.what}…`}
        {run.state === "done" && `Found ${run.found} more item${run.found === 1 ? "" : "s"} in ${(run.ms / 1000).toFixed(1)} s.`}
        {run.state === "failed" && <span className="error">{run.error}</span>}
        {run.state === "idle" && "Names and addresses are found automatically."}
      </p>
      <form
        className="ai-row"
        onSubmit={(e) => {
          e.preventDefault();
          if (lookFor.trim()) onLookFor(lookFor.trim());
          setLookFor("");
        }}
      >
        <label htmlFor="ai-also" className="visually-hidden">
          Also look for
        </label>
        <input
          id="ai-also"
          className="input"
          style={{ flex: 1, minWidth: 0 }}
          placeholder="Also look for, e.g. patient name"
          value={lookFor}
          onChange={(e) => setLookFor(e.target.value)}
        />
        <button type="submit" className="btn-ghost ai-btn" style={{ height: 40, borderRadius: 8 }} disabled={!lookFor.trim() || run.state === "running"}>
          Look
        </button>
      </form>
    </section>
  );
}
