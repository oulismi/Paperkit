// Downloads the font used for text added in Edit: Noto Sans Regular (SIL Open
// Font License 1.1, Noto project, Latin + Greek + Cyrillic). ~0.6 MB, bundled.
//
//   node scripts/fetch-fonts.mjs [target-dir]      (default: app/src-tauri/resources/fonts)
import { join, resolve } from "node:path";
import { fetchVerified } from "./lib/fetch-verified.mjs";

const COMMIT = "28b15b4b43b7bed62b5cf6e6b0b5ff5846270535";
const FILES = [
  {
    path: "NotoSans-Regular.ttf",
    size: 621_572,
    sha256: "478c558ea716033cd60c03438f628dfa75694dcf6b5f6d505a2f05fd2b4f3823",
    url: `https://raw.githubusercontent.com/notofonts/notofonts.github.io/${COMMIT}/fonts/NotoSans/hinted/ttf/NotoSans-Regular.ttf`,
  },
];

await fetchVerified(FILES, resolve(process.argv[2] ?? join(import.meta.dirname, "..", "app", "src-tauri", "resources", "fonts")));
