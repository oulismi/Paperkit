// Downloads Tesseract language data (tessdata_fast, Apache-2.0) for OCR of
// scanned pages: English, French, Dutch, Greek. ~12.7 MB, bundled with the app.
//
//   node scripts/fetch-tessdata.mjs [target-dir]   (default: app/src-tauri/resources/tessdata)
import { join, resolve } from "node:path";
import { fetchVerified } from "./lib/fetch-verified.mjs";

const COMMIT = "87416418657359cb625c412a48b6e1d6d41c29bd";
const FILES = [
  { path: "eng.traineddata", size: 4_113_088, sha256: "7d4322bd2a7749724879683fc3912cb542f19906c83bcc1a52132556427170b2" },
  { path: "fra.traineddata", size: 1_130_365, sha256: "ced037562e8c80c13122dece28dd477d399af80911a28791a66a63ac1e3445ca" },
  { path: "nld.traineddata", size: 6_050_296, sha256: "ced0e5e046a84c908a6aa7accbef9a232c4a5d9a8276691b81c6ee64d02963f6" },
  { path: "ell.traineddata", size: 1_419_514, sha256: "4fba8a0b461038d51f1c20d043d4f2ac38c4e778f1b90830847f7bd8fa3ba726" },
].map((f) => ({ ...f, url: `https://raw.githubusercontent.com/tesseract-ocr/tessdata_fast/${COMMIT}/${f.path}` }));

await fetchVerified(FILES, resolve(process.argv[2] ?? join(import.meta.dirname, "..", "app", "src-tauri", "resources", "tessdata")));
