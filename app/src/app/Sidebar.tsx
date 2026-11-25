import { Icon } from "../components/Icon";
import { TOOLS, type ToolId } from "./tools";

interface Props {
  current: "home" | ToolId;
  onNavigate: (to: "home" | ToolId) => void;
}

export function Sidebar({ current, onNavigate }: Props) {
  const item = (id: "home" | ToolId, label: string, icon: Parameters<typeof Icon>[0]["name"], extra?: React.ReactNode) => (
    <button
      key={id}
      type="button"
      className="nav-item"
      aria-current={current === id ? "page" : undefined}
      onClick={() => onNavigate(id)}
    >
      <Icon name={icon} />
      {label}
      {extra}
    </button>
  );

  return (
    <nav className="sidebar" aria-label="Tools">
      <div className="brand">
        <span className="brand-mark" aria-hidden="true">P</span>
        PaperKit
      </div>
      {item("home", "Home", "home")}
      {TOOLS.map((t) =>
        item(t.id, t.label, t.icon, t.ai ? <span className="pill">AI</span> : !t.ready && <span className="pill soon">Soon</span>),
      )}
      <div className="offline-note">
        <Icon name="lock" size={16} strokeWidth={1.8} />
        <span>
          <strong>Offline mode.</strong> Files never leave this computer.
        </span>
      </div>
    </nav>
  );
}
