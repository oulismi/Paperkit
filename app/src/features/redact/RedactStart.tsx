import { Icon } from "../../components/Icon";
import type { SourceFile } from "../../app/model";

interface Props {
  openFiles: SourceFile[];
  onPick: (file: SourceFile) => void;
  onBrowse: () => void;
}

export function RedactStart({ openFiles, onPick, onBrowse }: Props) {
  return (
    <div className="rd-start">
      <div>
        <h1>Redact a PDF</h1>
        <p className="muted" style={{ marginTop: 6, fontSize: 15 }}>
          PaperKit finds emails, phone numbers, IBANs, card and ID numbers and dates. You approve each one, then a new copy is
          saved with that content removed for good.
        </p>
      </div>
      <div className="dropzone">
        <div className="dropzone-icon" style={{ background: "var(--ai-soft)", color: "var(--ai)" }}>
          <Icon name="redact" size={30} strokeWidth={1.5} />
        </div>
        <div className="dropzone-title">Drop a PDF here to redact it</div>
        <button type="button" className="btn" onClick={onBrowse}>
          Browse files
        </button>
      </div>
      {openFiles.length > 0 && (
        <section>
          <h2 className="section-title">Or use an open file</h2>
          <div className="field">
            {openFiles.map((f) => (
              <button key={f.id} type="button" className="file-row" style={{ background: "var(--surface)", textAlign: "left" }} onClick={() => onPick(f)}>
                <span className="dot" style={{ background: f.color.strip }} />
                <span className="name">{f.name}</span>
                <span className="muted">{f.pageCount} p.</span>
              </button>
            ))}
          </div>
        </section>
      )}
    </div>
  );
}
