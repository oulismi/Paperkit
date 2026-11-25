import { Icon, type IconName } from "./Icon";
import type { SourceFile } from "../app/model";

interface Props {
  title: string;
  intro: string;
  icon: IconName;
  openFiles: SourceFile[];
  onPick: (file: SourceFile) => void;
  onBrowse: () => void;
}

/** Start screen of a one-file tool: drop or browse, or reuse a file that is already open. */
export function PickFile({ title, intro, icon, openFiles, onPick, onBrowse }: Props) {
  return (
    <div className="rd-start">
      <div>
        <h1>{title}</h1>
        <p className="muted" style={{ marginTop: 6, fontSize: 15 }}>
          {intro}
        </p>
      </div>
      <div className="dropzone">
        <div className="dropzone-icon">
          <Icon name={icon} size={30} strokeWidth={1.5} />
        </div>
        <div className="dropzone-title">Drop a PDF here</div>
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
