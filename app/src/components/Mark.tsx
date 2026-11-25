// PaperKit's mark: a page with a redacted line. Used by the drop bubble, the
// sidebar and (as app-icon.svg) the app and tray icons.
export function Mark({ size = 24, color = "currentColor" }: { size?: number; color?: string }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={color} strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M7 3h7l4 4v14H7z" />
      <path d="M14 3v4h4" />
      <path d="M9.5 11h5" />
      <rect x="9.2" y="14" width="6.6" height="2.6" rx="0.6" fill={color} stroke="none" />
    </svg>
  );
}
