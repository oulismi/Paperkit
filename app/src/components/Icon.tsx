// Stroke icons from the mockups, drawn on a 20×20 grid.
const paths = {
  home: <path d="M3 10l7-6 7 6v7H3z" />,
  split: (
    <>
      <circle cx="5" cy="5" r="2.5" />
      <circle cx="5" cy="15" r="2.5" />
      <path d="M7 6.5L17 15M7 13.5L17 5" />
    </>
  ),
  merge: (
    <>
      <rect x="3" y="3" width="8" height="10" rx="1" />
      <rect x="9" y="7" width="8" height="10" rx="1" />
    </>
  ),
  redact: (
    <>
      <rect x="3" y="4" width="14" height="12" rx="1" />
      <rect x="5.5" y="8.5" width="9" height="3" fill="currentColor" />
    </>
  ),
  edit: <path d="M13 3l4 4-9 9H4v-4z" />,
  convert: <path d="M4 7h12l-3-3M16 13H4l3 3" />,
  upload: <path d="M10 13V3M6 7l4-4 4 4M3 13v4h14v-4" />,
  lock: (
    <>
      <rect x="4" y="9" width="12" height="8" rx="1.5" />
      <path d="M7 9V6a3 3 0 016 0v3" />
    </>
  ),
  check: <path d="M4 10l4 4 8-8" />,
  rotateLeft: <path d="M4 4v4h4M4.5 8A6 6 0 1 1 4 11" />,
  rotateRight: <path d="M16 4v4h-4M15.5 8A6 6 0 1 0 16 11" />,
  trash: <path d="M4 6h12M8 6V4h4v2M6 6l1 11h6l1-11" />,
  close: <path d="M5 5l10 10M15 5L5 15" />,
  up: <path d="M5 12l5-5 5 5" />,
  down: <path d="M5 8l5 5 5-5" />,
  extract: <path d="M7 3H4v14h12v-3M10 10l7-7M12 3h5v5" />,
};

export type IconName = keyof typeof paths;

export function Icon({ name, size = 18, strokeWidth = 1.6 }: { name: IconName; size?: number; strokeWidth?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 20 20"
      fill="none"
      stroke="currentColor"
      strokeWidth={strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {paths[name]}
    </svg>
  );
}
