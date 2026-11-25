import { Icon } from "../../components/Icon";
import { TOOLS, type ToolId } from "../../app/tools";

interface Props {
  onBrowse: () => void;
  onTool: (tool: ToolId) => void;
}

export function Home({ onBrowse, onTool }: Props) {
  return (
    <div className="home">
      <div>
        <h1>Drop a PDF to get started</h1>
        <p className="lead">Choose a tool after dropping, or pick one first.</p>
      </div>

      <div className="dropzone">
        <div className="dropzone-icon">
          <Icon name="upload" size={30} strokeWidth={1.5} />
        </div>
        <div className="dropzone-title">Drag &amp; drop PDF files here</div>
        <div className="muted">One file to split or reorder · several files to merge</div>
        <button type="button" className="btn" style={{ marginTop: 6 }} onClick={onBrowse}>
          Browse files
        </button>
      </div>

      <section aria-labelledby="tools-title">
        <h2 id="tools-title" className="section-title">Tools</h2>
        <div className="tools">
          {TOOLS.map((t) => (
            <button key={t.id} type="button" className={`tool-card${t.ai ? " ai" : ""}`} onClick={() => onTool(t.id)}>
              <span className="head" style={{ color: t.ai ? "var(--ai)" : "var(--accent)" }}>
                <Icon name={t.icon} size={22} />
                {t.ai ? <span className="pill">On-device AI</span> : !t.ready && <span className="pill soon">Coming soon</span>}
              </span>
              <span className="title">{t.label}</span>
              <span className="desc">{t.description}</span>
            </button>
          ))}
        </div>
      </section>
    </div>
  );
}
